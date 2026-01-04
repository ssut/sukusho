//! Clipboard operations for copying files
//!
//! Implements CF_HDROP format for copying file paths to clipboard.

use log::{debug, error, info};
use std::path::PathBuf;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{BOOL, HANDLE, POINT},
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND},
        Ole::CF_HDROP,
    },
    UI::Shell::DROPFILES,
};

/// Copy files to clipboard using CF_HDROP format
/// This allows pasting files in Explorer and other applications
#[cfg(windows)]
pub fn copy_files_to_clipboard(files: &[PathBuf]) -> bool {
    if files.is_empty() {
        return false;
    }

    info!("Copying {} files to clipboard", files.len());
    for file in files {
        debug!("  - {:?}", file);
    }

    unsafe {
        // Open clipboard
        if OpenClipboard(None).is_err() {
            error!("Failed to open clipboard");
            return false;
        }

        // Empty clipboard
        if EmptyClipboard().is_err() {
            error!("Failed to empty clipboard");
            let _ = CloseClipboard();
            return false;
        }

        // Create HDROP data
        let hdrop = match create_hdrop(files) {
            Some(h) => h,
            None => {
                error!("Failed to create HDROP data");
                let _ = CloseClipboard();
                return false;
            }
        };

        // Set clipboard data
        let result = SetClipboardData(CF_HDROP.0 as u32, hdrop);
        let success = result.is_ok();

        if success {
            info!("Successfully copied files to clipboard");
        } else {
            error!("Failed to set clipboard data: {:?}", result);
        }

        let _ = CloseClipboard();
        success
    }
}

/// Create DROPFILES structure in global memory
#[cfg(windows)]
unsafe fn create_hdrop(files: &[PathBuf]) -> Option<HANDLE> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    // Calculate total size needed
    let mut total_size = std::mem::size_of::<DROPFILES>();

    // Add size for each file path (wide chars + null terminator)
    let wide_paths: Vec<Vec<u16>> = files
        .iter()
        .map(|p| {
            OsStr::new(p)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        })
        .collect();

    for wide_path in &wide_paths {
        total_size += wide_path.len() * 2;
    }
    total_size += 2; // Double null terminator at end

    // Allocate global memory
    // SAFETY: Allocating global memory for clipboard data
    let hglobal = unsafe { GlobalAlloc(GHND, total_size).ok()? };
    // SAFETY: Locking global memory to write data
    let ptr = unsafe { GlobalLock(hglobal) };
    if ptr.is_null() {
        return None;
    }

    // Fill DROPFILES structure
    let dropfiles = ptr as *mut DROPFILES;
    // SAFETY: Writing to allocated memory
    unsafe {
        (*dropfiles).pFiles = std::mem::size_of::<DROPFILES>() as u32;
        (*dropfiles).pt = POINT { x: 0, y: 0 };
        (*dropfiles).fNC = BOOL::from(false);
        (*dropfiles).fWide = BOOL::from(true); // Wide characters
    }

    // Copy file paths after DROPFILES structure
    // SAFETY: Calculating offset into allocated memory
    let mut dest = unsafe { (ptr as *mut u8).add(std::mem::size_of::<DROPFILES>()) as *mut u16 };

    for wide_path in &wide_paths {
        // SAFETY: Copying path data to global memory
        unsafe {
            std::ptr::copy_nonoverlapping(wide_path.as_ptr(), dest, wide_path.len());
            dest = dest.add(wide_path.len());
        }
    }

    // Double null terminator
    // SAFETY: Writing final null terminator
    unsafe { *dest = 0 };

    // SAFETY: Unlocking global memory
    let _ = unsafe { GlobalUnlock(hglobal) };

    Some(HANDLE(hglobal.0))
}

#[cfg(not(windows))]
pub fn copy_files_to_clipboard(_files: &[PathBuf]) -> bool {
    false
}
