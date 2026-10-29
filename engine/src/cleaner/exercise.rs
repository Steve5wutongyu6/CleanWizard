//! Executors: the only place the engine touches the file system destructively.

use std::path::{Path, PathBuf};

/// Something that can move a path to the trash.
pub trait Executor {
    fn trash(&self, path: &Path) -> Result<(), String>;
}

/// Test double: records requests, never deletes.
#[derive(Debug, Default)]
pub struct RecordingExecutor {
    pub requested: std::sync::Mutex<Vec<PathBuf>>,
    pub fail_on: std::sync::Mutex<Vec<PathBuf>>,
}

impl RecordingExecutor {
    pub fn new() -> RecordingExecutor {
        RecordingExecutor::default()
    }
    pub fn fail_for(&self, path: &Path) {
        self.fail_on.lock().unwrap_or_else(|e| e.into_inner()).push(path.to_path_buf());
    }
    pub fn requests(&self) -> Vec<PathBuf> {
        self.requested
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl Executor for RecordingExecutor {
    fn trash(&self, path: &Path) -> Result<(), String> {
        let fail = self
            .fail_on
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&path.to_path_buf());
        self.requested
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(path.to_path_buf());
        if fail {
            Err("simulated failure".to_string())
        } else {
            Ok(())
        }
    }
}

/// Moves paths into `~/.Trash`, mirroring `NSFileManager.trashItem`.
#[derive(Debug, Default)]
pub struct TrashExecutor;

impl Executor for TrashExecutor {
    fn trash(&self, path: &Path) -> Result<(), String> {
        remove_to_trash(path)
    }
}

/// Rename into the trash directory, de-duplicating the target name.
pub fn remove_to_trash(path: &Path) -> Result<(), String> {
    let trash = crate::support::fsutil::home_dir()
        .ok_or_else(|| "home directory unavailable".to_string())?
        .join(".Trash");
    std::fs::create_dir_all(&trash).map_err(|e| e.to_string())?;

    let name = path
        .file_name()
        .ok_or_else(|| "path has no file name".to_string())?;
    let mut target = trash.join(name);
    let mut suffix = 1u32;
    while target.exists() {
        let mut alt = name.to_os_string();
        alt.push(format!(" {suffix}"));
        target = trash.join(alt);
        suffix += 1;
        if suffix > 9999 {
            return Err("trash name space exhausted".to_string());
        }
    }

    match std::fs::rename(path, &target) {
        Ok(()) => Ok(()),
        // Cross-device moves cannot be renamed; fall back to copy + remove.
        Err(_) => {
            if path.is_dir() {
                copy_dir(path, &target)?;
                std::fs::remove_dir_all(path).map_err(|e| e.to_string())
            } else {
                std::fs::copy(path, &target).map_err(|e| e.to_string())?;
                std::fs::remove_file(path).map_err(|e| e.to_string())
            }
        }
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
