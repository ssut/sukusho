//! Native Windows drag-and-drop implementation using Windows COM APIs directly
//!
//! Implements IDataObject and IDropSource for OLE drag-drop operations.

use log::{debug, error, info};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Flag to prevent multiple concurrent drag operations
static DRAG_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Simple drag threshold check (for use with separate start_drag call)
#[cfg(windows)]
pub fn check_drag_threshold() -> bool {
    use crate::tray::WINDOW_HWND;
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::UI::Input::KeyboardAndMouse::DragDetect;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_err() {
            return false;
        }

        let hwnd = match *WINDOW_HWND.lock() {
            Some(h) => HWND(h as *mut std::ffi::c_void),
            None => return false,
        };

        DragDetect(hwnd, pt).as_bool()
    }
}

#[cfg(not(windows))]
pub fn check_drag_threshold() -> bool {
    false
}

// Windows HRESULT constants for drag-drop
#[cfg(windows)]
const DRAGDROP_S_DROP: i32 = 0x00040100;
#[cfg(windows)]
const DRAGDROP_S_CANCEL: i32 = 0x00040101;
#[cfg(windows)]
const DRAGDROP_S_USEDEFAULTCURSORS: i32 = 0x00040102;

// Mouse button flag
#[cfg(windows)]
const MK_LBUTTON: u32 = 0x0001;

// Error codes
#[cfg(windows)]
const DV_E_FORMATETC: i32 = -2147221404i32; // 0x80040064
#[cfg(windows)]
const DATA_S_SAMEFORMATETC: i32 = 0x00040130;

/// Start a drag operation with the given files
/// Returns true if drag actually happened, false if user just clicked (or error)
#[cfg(windows)]
pub fn start_drag(files: &[PathBuf]) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{implement, HRESULT};
    use windows::Win32::Foundation::{BOOL, E_NOTIMPL, S_OK};
    use windows::Win32::System::Com::{
        IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, DATADIR_GET,
        DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL,
    };
    use windows::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
    };
    use windows::Win32::System::Ole::{
        DoDragDrop, IDropSource, IDropSource_Impl, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY,
        DROPEFFECT_NONE,
    };
    use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
    use windows::Win32::UI::Shell::{SHCreateStdEnumFmtEtc, DROPFILES};

    if files.is_empty() {
        info!("start_drag called with empty files list");
        return false;
    }

    // Check if drag is already in progress
    if DRAG_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        info!("Drag already in progress, skipping - resetting state");
        // Reset the flag in case it got stuck
        DRAG_IN_PROGRESS.store(false, Ordering::SeqCst);
        return false;
    }

    // Use a guard to ensure DRAG_IN_PROGRESS is reset even if we panic
    struct DragGuard;
    impl Drop for DragGuard {
        fn drop(&mut self) {
            DRAG_IN_PROGRESS.store(false, Ordering::SeqCst);
            info!("Drag guard dropped, state reset");
        }
    }
    let _guard = DragGuard;

    info!(
        "=== Starting native drag operation with {} files ===",
        files.len()
    );

    // Normalize paths - avoid \\?\ prefix which some apps don't handle
    let normalized_paths: Vec<PathBuf> = files
        .iter()
        .filter_map(|p| {
            // Get absolute path without the \\?\ prefix
            let path = if p.is_absolute() {
                p.clone()
            } else {
                std::fs::canonicalize(p).ok()?
            };

            // Strip \\?\ prefix if present
            let path_str = path.to_string_lossy();
            if path_str.starts_with(r"\\?\") {
                Some(PathBuf::from(&path_str[4..]))
            } else {
                Some(path)
            }
        })
        .filter(|p| p.exists())
        .collect();

    if normalized_paths.is_empty() {
        error!("No valid paths for drag operation");
        DRAG_IN_PROGRESS.store(false, Ordering::SeqCst);
        return false;
    }

    for path in &normalized_paths {
        info!("  File: {:?}", path);
    }

    // Implement IDataObject - Explorer requires proper EnumFormatEtc
    #[implement(IDataObject)]
    struct FileDataObject {
        paths: Vec<PathBuf>,
    }

    impl IDataObject_Impl for FileDataObject_Impl {
        fn GetData(&self, pformatetc: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
            unsafe {
                let fmt = &*pformatetc;

                info!(
                    "GetData called: cfFormat={}, tymed={}",
                    fmt.cfFormat, fmt.tymed
                );

                // Only support CF_HDROP format with HGLOBAL
                if fmt.cfFormat != CF_HDROP.0 {
                    info!(
                        "GetData: wrong format {}, expected {}",
                        fmt.cfFormat, CF_HDROP.0
                    );
                    return Err(windows::core::Error::from_hresult(HRESULT(DV_E_FORMATETC)));
                }

                if (fmt.tymed & TYMED_HGLOBAL.0 as u32) == 0 {
                    info!("GetData: wrong tymed {}, expected HGLOBAL", fmt.tymed);
                    return Err(windows::core::Error::from_hresult(HRESULT(DV_E_FORMATETC)));
                }

                // Build the file list as wide strings (UTF-16)
                let mut wide_buffer: Vec<u16> = Vec::new();
                for path in &self.paths {
                    let path_str = path.to_string_lossy();
                    debug!("GetData: encoding path: {}", path_str);
                    let wide: Vec<u16> = OsStr::new(path)
                        .encode_wide()
                        .chain(std::iter::once(0)) // null terminator for each path
                        .collect();
                    debug!("GetData: path encoded to {} u16 chars", wide.len());
                    wide_buffer.extend_from_slice(&wide);
                }
                wide_buffer.push(0); // Double null terminator at end

                let header_size = std::mem::size_of::<DROPFILES>();
                let data_size = wide_buffer.len() * 2; // 2 bytes per u16
                let total_size = header_size + data_size;

                debug!("GetData: DROPFILES header size: {} bytes", header_size);
                debug!(
                    "GetData: Total wide_buffer length: {} u16 chars ({} bytes)",
                    wide_buffer.len(),
                    data_size
                );
                info!(
                    "GetData: allocating {} bytes (header={}, data={})",
                    total_size, header_size, data_size
                );

                // Allocate global memory
                let hglobal = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total_size)?;
                let ptr = GlobalLock(hglobal);
                if ptr.is_null() {
                    error!("GetData: GlobalLock failed");
                    return Err(windows::core::Error::from_hresult(HRESULT(E_NOTIMPL.0)));
                }

                // Fill DROPFILES header
                let dropfiles = ptr as *mut DROPFILES;
                (*dropfiles).pFiles = header_size as u32;
                (*dropfiles).fWide = BOOL(1); // UTF-16

                // Copy file paths after header
                let data_ptr = (ptr as *mut u8).add(header_size) as *mut u16;
                std::ptr::copy_nonoverlapping(wide_buffer.as_ptr(), data_ptr, wide_buffer.len());

                let _ = GlobalUnlock(hglobal);

                info!("GetData: success, returning STGMEDIUM");

                Ok(STGMEDIUM {
                    tymed: TYMED_HGLOBAL.0 as u32,
                    u: std::mem::transmute(hglobal),
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                })
            }
        }

        fn GetDataHere(
            &self,
            _pformatetc: *const FORMATETC,
            _pmedium: *mut STGMEDIUM,
        ) -> windows::core::Result<()> {
            Err(windows::core::Error::from_hresult(HRESULT(E_NOTIMPL.0)))
        }

        fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
            unsafe {
                let fmt = &*pformatetc;
                info!(
                    "QueryGetData: cfFormat={}, tymed={}",
                    fmt.cfFormat, fmt.tymed
                );

                if fmt.cfFormat == CF_HDROP.0 && (fmt.tymed & TYMED_HGLOBAL.0 as u32) != 0 {
                    info!("QueryGetData: S_OK");
                    S_OK
                } else {
                    info!("QueryGetData: DV_E_FORMATETC");
                    HRESULT(DV_E_FORMATETC)
                }
            }
        }

        fn GetCanonicalFormatEtc(
            &self,
            _pformatectin: *const FORMATETC,
            _pformatetcout: *mut FORMATETC,
        ) -> HRESULT {
            HRESULT(DATA_S_SAMEFORMATETC)
        }

        fn SetData(
            &self,
            _pformatetc: *const FORMATETC,
            _pmedium: *const STGMEDIUM,
            _frelease: BOOL,
        ) -> windows::core::Result<()> {
            Err(windows::core::Error::from_hresult(HRESULT(E_NOTIMPL.0)))
        }

        fn EnumFormatEtc(&self, dwdirection: u32) -> windows::core::Result<IEnumFORMATETC> {
            info!("EnumFormatEtc called: direction={}", dwdirection);

            if dwdirection == DATADIR_GET.0 as u32 {
                // Create standard format enumerator using Shell helper function
                let formats = [FORMATETC {
                    cfFormat: CF_HDROP.0,
                    ptd: std::ptr::null_mut(),
                    dwAspect: DVASPECT_CONTENT.0 as u32,
                    lindex: -1,
                    tymed: TYMED_HGLOBAL.0 as u32,
                }];

                unsafe {
                    let result = SHCreateStdEnumFmtEtc(&formats);
                    info!(
                        "EnumFormatEtc: SHCreateStdEnumFmtEtc result: {:?}",
                        result.is_ok()
                    );
                    result
                }
            } else {
                Err(windows::core::Error::from_hresult(HRESULT(E_NOTIMPL.0)))
            }
        }

        fn DAdvise(
            &self,
            _pformatetc: *const FORMATETC,
            _advf: u32,
            _padvsink: Option<&IAdviseSink>,
        ) -> windows::core::Result<u32> {
            Err(windows::core::Error::from_hresult(HRESULT(E_NOTIMPL.0)))
        }

        fn DUnadvise(&self, _dwconnection: u32) -> windows::core::Result<()> {
            Err(windows::core::Error::from_hresult(HRESULT(E_NOTIMPL.0)))
        }

        fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
            Err(windows::core::Error::from_hresult(HRESULT(E_NOTIMPL.0)))
        }
    }

    // Implement IDropSource
    #[implement(IDropSource)]
    struct FileDropSource;

    impl IDropSource_Impl for FileDropSource_Impl {
        fn QueryContinueDrag(
            &self,
            fescapepressed: BOOL,
            grfkeystate: MODIFIERKEYS_FLAGS,
        ) -> HRESULT {
            if fescapepressed.as_bool() {
                info!("QueryContinueDrag: ESC pressed, canceling");
                return HRESULT(DRAGDROP_S_CANCEL);
            }

            // If left mouse button is released, drop
            if grfkeystate.0 & MK_LBUTTON == 0 {
                info!("QueryContinueDrag: mouse released, dropping");
                return HRESULT(DRAGDROP_S_DROP);
            }

            S_OK
        }

        fn GiveFeedback(&self, dweffect: DROPEFFECT) -> HRESULT {
            info!("GiveFeedback: effect={:?}", dweffect);
            HRESULT(DRAGDROP_S_USEDEFAULTCURSORS)
        }
    }

    // Create COM objects
    let data_object: IDataObject = FileDataObject {
        paths: normalized_paths,
    }
    .into();
    let drop_source: IDropSource = FileDropSource.into();

    info!("Calling DoDragDrop...");

    // Call DoDragDrop - this is a blocking modal loop
    // Only allow COPY to prevent files from being moved/deleted from the screenshot folder
    let mut drop_effect = DROPEFFECT_NONE;
    let result = unsafe {
        DoDragDrop(
            &data_object,
            &drop_source,
            DROPEFFECT_COPY, // Only COPY, not MOVE - we don't want files moved away
            &mut drop_effect,
        )
    };

    DRAG_IN_PROGRESS.store(false, Ordering::SeqCst);

    info!(
        "=== DoDragDrop returned: result={:?}, effect={:?} ===",
        result, drop_effect
    );

    // DoDragDrop returns HRESULT:
    // - DRAGDROP_S_DROP (0x00040100): Drop was successful
    // - DRAGDROP_S_CANCEL (0x00040101): User cancelled (ESC or just clicked without dragging)
    // - S_OK: Also indicates success
    //
    // Return true only if an actual drag happened (not cancelled)
    // This allows the caller to handle clicks separately
    if result.0 == DRAGDROP_S_DROP || (result.is_ok() && drop_effect != DROPEFFECT_NONE) {
        info!("Drag was completed successfully");
        true
    } else if result.0 == DRAGDROP_S_CANCEL {
        info!("Drag was cancelled (user clicked without dragging or pressed ESC)");
        false
    } else if result.is_err() {
        error!("DoDragDrop failed: {:?}", result);
        false
    } else {
        info!("Drag ended with no effect");
        false
    }
}

#[cfg(not(windows))]
pub fn start_drag(_files: &[PathBuf]) -> bool {
    false
}
