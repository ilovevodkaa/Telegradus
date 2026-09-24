//! Download state of files and cached image handles.
//!
//! An `image::Handle` must be reused between frames: a fresh handle would make
//! the renderer decode the file again. Handles are created once per file when
//! its download completes.

use std::collections::HashMap;

use iced::widget::image;
use telegradus_core::{FileId, FileRef};

#[derive(Debug)]
struct Entry {
    file: FileRef,
    /// Set for files shown as images (avatars, photos) once downloaded.
    handle: Option<image::Handle>,
    is_image: bool,
    /// A `DownloadFile` was sent and has not failed since.
    requested: bool,
}

/// Every file the UI has seen, keyed by TDLib file id.
#[derive(Debug, Default)]
pub struct Files {
    entries: HashMap<FileId, Entry>,
}

impl Files {
    /// Records a file reference from a chat or message. Keeps the most
    /// advanced state when the file is already known.
    pub fn register(&mut self, file: &FileRef, is_image: bool) {
        let entry = self.entries.entry(file.id).or_insert_with(|| Entry {
            file: file.clone(),
            handle: None,
            is_image,
            requested: false,
        });
        entry.is_image |= is_image;
        if !entry.file.is_downloaded()
            && (file.is_downloaded() || file.downloaded_size > entry.file.downloaded_size)
        {
            entry.file = file.clone();
        }
        entry.refresh_handle();
    }

    /// Applies a download progress or completion update.
    pub fn update(&mut self, file: FileRef) {
        let entry = self.entries.entry(file.id).or_insert_with(|| Entry {
            file: file.clone(),
            handle: None,
            is_image: false,
            requested: false,
        });
        if entry.file.local_path != file.local_path {
            entry.handle = None;
        }
        entry.file = file;
        entry.refresh_handle();
    }

    /// Marks the file as requested and returns `true` if a `DownloadFile`
    /// command should be sent now.
    pub fn request(&mut self, id: FileId) -> bool {
        match self.entries.get_mut(&id) {
            Some(entry) if !entry.requested && !entry.file.is_downloaded() => {
                entry.requested = true;
                true
            }
            Some(_) => false,
            None => {
                self.entries.insert(
                    id,
                    Entry {
                        file: FileRef {
                            id,
                            size: 0,
                            local_path: None,
                            is_downloading: false,
                            downloaded_size: 0,
                        },
                        handle: None,
                        is_image: false,
                        requested: true,
                    },
                );
                true
            }
        }
    }

    /// Allows a failed download to be requested again.
    pub fn clear_requests(&mut self) {
        for entry in self.entries.values_mut() {
            if !entry.file.is_downloaded() && !entry.file.is_downloading {
                entry.requested = false;
            }
        }
    }

    pub fn get(&self, id: FileId) -> Option<&FileRef> {
        self.entries.get(&id).map(|e| &e.file)
    }

    /// The cached image handle of a downloaded image file.
    pub fn image(&self, id: FileId) -> Option<&image::Handle> {
        self.entries.get(&id).and_then(|e| e.handle.as_ref())
    }

    pub fn is_requested(&self, id: FileId) -> bool {
        self.entries.get(&id).is_some_and(|e| e.requested)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

impl Entry {
    fn refresh_handle(&mut self) {
        if !self.is_image || self.handle.is_some() {
            return;
        }
        if let Some(path) = &self.file.local_path {
            self.handle = Some(image::Handle::from_path(path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(id: FileId, path: Option<&str>, downloaded: i64) -> FileRef {
        FileRef {
            id,
            size: 100,
            local_path: path.map(PathBuf::from),
            is_downloading: path.is_none() && downloaded > 0,
            downloaded_size: downloaded,
        }
    }

    #[test]
    fn request_once() {
        let mut files = Files::default();
        files.register(&file(1, None, 0), true);
        assert!(files.request(1));
        assert!(!files.request(1));
        // Unknown files can be requested too.
        assert!(files.request(2));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn downloaded_files_are_not_requested() {
        let mut files = Files::default();
        files.register(&file(1, Some("/tmp/a.jpg"), 100), true);
        assert!(!files.request(1));
        assert!(files.image(1).is_some());
    }

    #[test]
    fn handle_is_created_once_and_kept() {
        let mut files = Files::default();
        files.register(&file(1, None, 0), true);
        assert!(files.image(1).is_none());
        files.update(file(1, None, 50));
        assert!(files.image(1).is_none());
        files.update(file(1, Some("/tmp/a.jpg"), 100));
        let first = files.image(1).cloned();
        assert!(first.is_some());
        // A stale registration does not reset the downloaded state.
        files.register(&file(1, None, 0), true);
        assert_eq!(files.image(1).cloned(), first);
        assert!(files.get(1).is_some_and(FileRef::is_downloaded));
    }

    #[test]
    fn non_images_have_no_handle() {
        let mut files = Files::default();
        files.register(&file(3, None, 0), false);
        files.update(file(3, Some("/tmp/doc.pdf"), 100));
        assert!(files.image(3).is_none());
    }

    #[test]
    fn failed_requests_can_retry() {
        let mut files = Files::default();
        files.register(&file(1, None, 0), true);
        assert!(files.request(1));
        files.clear_requests();
        assert!(files.request(1));
    }
}
