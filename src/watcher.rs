//! File system watcher for screenshot directory

use anyhow::Result;
use crossbeam_channel::Sender;
use log::{debug, error, info, warn};
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::AppMessage;

/// Image extensions we care about
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "avif"];

pub struct ScreenshotWatcher {
    directory: PathBuf,
    message_tx: Sender<AppMessage>,
}

impl ScreenshotWatcher {
    pub fn new(directory: PathBuf, message_tx: Sender<AppMessage>) -> Self {
        Self {
            directory,
            message_tx,
        }
    }

    /// Run the watcher (blocking)
    pub fn run(self) -> Result<()> {
        info!("Starting file watcher for: {:?}", self.directory);

        // Ensure directory exists
        if !self.directory.exists() {
            warn!(
                "Screenshot directory does not exist, creating: {:?}",
                self.directory
            );
            std::fs::create_dir_all(&self.directory)?;
        }

        // Scan existing files first
        self.scan_existing_files()?;

        // Create debounced watcher
        let tx = self.message_tx.clone();
        let mut debouncer = new_debouncer(
            Duration::from_millis(200),
            None,
            move |result: DebounceEventResult| {
                Self::handle_debounced_events(result, &tx);
            },
        )?;

        // Watch the directory
        debouncer.watch(&self.directory, RecursiveMode::NonRecursive)?;

        info!("File watcher started successfully");

        // Keep the thread alive
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }

    /// Scan existing files in the directory
    fn scan_existing_files(&self) -> Result<()> {
        info!("Scanning existing screenshots...");
        let mut count = 0;

        if let Ok(entries) = std::fs::read_dir(&self.directory) {
            // Collect and sort by modified time (newest first)
            let mut files: Vec<_> = entries
                .flatten()
                .filter(|e| Self::is_image_file(&e.path()))
                .collect();

            files.sort_by(|a, b| {
                let a_time = a.metadata().and_then(|m| m.modified()).ok();
                let b_time = b.metadata().and_then(|m| m.modified()).ok();
                b_time.cmp(&a_time)
            });

            for entry in files {
                let path = entry.path();
                debug!("Found existing screenshot: {:?}", path);
                let _ = self.message_tx.send(AppMessage::NewScreenshot(path));
                count += 1;
            }
        }

        info!("Found {} existing screenshots", count);
        Ok(())
    }

    /// Handle debounced file system events
    fn handle_debounced_events(result: DebounceEventResult, tx: &Sender<AppMessage>) {
        match result {
            Ok(events) => {
                for event in events {
                    Self::process_event(&event, tx);
                }
            }
            Err(errors) => {
                for e in errors {
                    error!("File watcher error: {:?}", e);
                }
            }
        }
    }

    /// Process a single debounced event
    fn process_event(event: &notify_debouncer_full::DebouncedEvent, tx: &Sender<AppMessage>) {
        use notify::EventKind;

        for path in &event.paths {
            if !Self::is_image_file(path) {
                continue;
            }

            match &event.kind {
                EventKind::Create(_) => {
                    info!("New screenshot detected: {:?}", path);
                    let _ = tx.send(AppMessage::NewScreenshot(path.clone()));
                }
                EventKind::Remove(_) => {
                    info!("Screenshot removed: {:?}", path);
                    let _ = tx.send(AppMessage::ScreenshotRemoved(path.clone()));
                }
                EventKind::Modify(_) => {
                    // Modification might mean the file is fully written
                    debug!("Screenshot modified: {:?}", path);
                }
                _ => {}
            }
        }
    }

    /// Check if a path is an image file we care about
    fn is_image_file(path: &Path) -> bool {
        if !path.is_file() {
            return false;
        }

        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                IMAGE_EXTENSIONS
                    .iter()
                    .any(|&e| e.eq_ignore_ascii_case(ext))
            })
    }
}
