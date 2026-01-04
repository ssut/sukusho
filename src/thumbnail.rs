//! Windows thumbnail extraction using Shell APIs
//!
//! Note: Currently images are loaded directly by gpui. This module provides
//! thumbnail caching infrastructure for future optimization.

#![allow(dead_code)]

use image::{DynamicImage, RgbaImage};
use log::{debug, warn};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(windows)]
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::SIZE,
        Graphics::Gdi::{
            CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, SelectObject, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
        },
        UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY},
    },
};

/// Default thumbnail size
pub const THUMBNAIL_SIZE: u32 = 150;

/// Thumbnail cache to avoid regenerating thumbnails
pub struct ThumbnailCache {
    /// Path -> RGBA image data
    cache: Mutex<HashMap<PathBuf, Arc<RgbaImage>>>,
    /// Maximum cache size
    max_size: usize,
}

impl ThumbnailCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            max_size,
        }
    }

    /// Get a cached thumbnail or generate a new one
    pub fn get_or_create(&self, path: &Path, size: u32) -> Option<Arc<RgbaImage>> {
        // Check cache first
        {
            let cache = self.cache.lock();
            if let Some(img) = cache.get(path) {
                return Some(Arc::clone(img));
            }
        }

        // Generate thumbnail
        let img = self.generate_thumbnail(path, size)?;
        let img = Arc::new(img);

        // Store in cache
        {
            let mut cache = self.cache.lock();

            // Evict oldest entries if cache is full
            if cache.len() >= self.max_size {
                // Simple eviction: remove first entry
                if let Some(key) = cache.keys().next().cloned() {
                    cache.remove(&key);
                }
            }

            cache.insert(path.to_path_buf(), Arc::clone(&img));
        }

        Some(img)
    }

    /// Remove a path from the cache
    pub fn invalidate(&self, path: &Path) {
        let mut cache = self.cache.lock();
        cache.remove(path);
    }

    /// Clear all cached thumbnails
    pub fn clear(&self) {
        let mut cache = self.cache.lock();
        cache.clear();
    }

    /// Generate a thumbnail for the given path
    fn generate_thumbnail(&self, path: &Path, size: u32) -> Option<RgbaImage> {
        // Try Windows Shell API first (fastest, uses system cache)
        #[cfg(windows)]
        if let Some(img) = self.get_windows_thumbnail(path, size) {
            return Some(img);
        }

        // Fall back to manual thumbnail generation
        self.generate_manual_thumbnail(path, size)
    }

    /// Get thumbnail using Windows Shell API
    #[cfg(windows)]
    fn get_windows_thumbnail(&self, path: &Path, size: u32) -> Option<RgbaImage> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        unsafe {
            // Convert path to wide string
            let wide_path: Vec<u16> = OsStr::new(path)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            // Create shell item
            let shell_item: IShellItemImageFactory =
                match SHCreateItemFromParsingName(PCWSTR(wide_path.as_ptr()), None) {
                    Ok(item) => item,
                    Err(e) => {
                        debug!("Failed to create shell item for {:?}: {}", path, e);
                        return None;
                    }
                };

            // Get thumbnail
            let hbitmap: HBITMAP = match shell_item.GetImage(
                SIZE {
                    cx: size as i32,
                    cy: size as i32,
                },
                SIIGBF_THUMBNAILONLY,
            ) {
                Ok(bmp) => bmp,
                Err(e) => {
                    debug!("Failed to get thumbnail for {:?}: {}", path, e);
                    return None;
                }
            };

            // Convert HBITMAP to RgbaImage
            let result = self.hbitmap_to_rgba(hbitmap, size);

            // Clean up
            let _ = DeleteObject(hbitmap);

            result
        }
    }

    /// Convert Windows HBITMAP to RgbaImage
    #[cfg(windows)]
    unsafe fn hbitmap_to_rgba(&self, hbitmap: HBITMAP, size: u32) -> Option<RgbaImage> {
        let hdc = unsafe { CreateCompatibleDC(None) };
        if hdc.is_invalid() {
            return None;
        }

        let _old = unsafe { SelectObject(hdc, hbitmap) };

        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32), // Negative for top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut buffer = vec![0u8; (size * size * 4) as usize];

        let result = unsafe {
            GetDIBits(
                hdc,
                hbitmap,
                0,
                size,
                Some(buffer.as_mut_ptr() as *mut _),
                &mut bi,
                DIB_RGB_COLORS,
            )
        };

        let _ = unsafe { DeleteDC(hdc) };

        if result == 0 {
            return None;
        }

        // Convert BGRA to RGBA
        for chunk in buffer.chunks_exact_mut(4) {
            chunk.swap(0, 2); // Swap B and R
        }

        RgbaImage::from_raw(size, size, buffer)
    }

    /// Manual thumbnail generation using image crate
    fn generate_manual_thumbnail(&self, path: &Path, size: u32) -> Option<RgbaImage> {
        debug!("Generating manual thumbnail for {:?}", path);

        // Load image
        let img = match image::open(path) {
            Ok(img) => img,
            Err(e) => {
                warn!("Failed to open image {:?}: {}", path, e);
                return None;
            }
        };

        // Use fast_image_resize for better performance
        Some(self.resize_with_fast_image_resize(&img, size))
    }

    /// Resize image using fast_image_resize crate
    fn resize_with_fast_image_resize(&self, img: &DynamicImage, target_size: u32) -> RgbaImage {
        use fast_image_resize::{images::Image, ResizeAlg, ResizeOptions, Resizer};

        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();

        // Calculate aspect-ratio-preserving dimensions
        let (new_width, new_height) = if width > height {
            let ratio = target_size as f32 / width as f32;
            (target_size, (height as f32 * ratio) as u32)
        } else {
            let ratio = target_size as f32 / height as f32;
            ((width as f32 * ratio) as u32, target_size)
        };

        let new_width = new_width.max(1);
        let new_height = new_height.max(1);

        // Create source image
        let src_image = Image::from_vec_u8(
            width,
            height,
            rgba.into_raw(),
            fast_image_resize::PixelType::U8x4,
        )
        .expect("Failed to create source image");

        // Create destination image
        let mut dst_image = Image::new(new_width, new_height, fast_image_resize::PixelType::U8x4);

        // Resize
        let mut resizer = Resizer::new();
        let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(
            fast_image_resize::FilterType::Lanczos3,
        ));

        resizer
            .resize(&src_image, &mut dst_image, &options)
            .expect("Failed to resize image");

        RgbaImage::from_raw(new_width, new_height, dst_image.into_vec())
            .unwrap_or_else(|| RgbaImage::new(new_width, new_height))
    }
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        Self::new(500)
    }
}
