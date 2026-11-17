//! A record this adapter keeps in its own configuration directory: owner-only, written
//! whole through a staged file, and read as nothing kept where it is not there.

use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

const OWNER_ONLY: u32 = 0o600;

const STAGED: &str = ".staged";

/// One record, by the file it is kept in.
#[derive(Debug, Clone)]
pub(in crate::plex) struct Record {
    at: PathBuf,
    file: &'static str,
}

impl Record {
    /// The record `file` in the configuration directory `directory`.
    pub(in crate::plex) fn in_directory(directory: &Path, file: &'static str) -> Self {
        Self {
            at: directory.to_path_buf(),
            file,
        }
    }

    /// What is kept: the default where nothing was ever kept.
    ///
    /// # Errors
    ///
    /// Why the record could not be read, where it is there and unreadable.
    pub(in crate::plex) fn read<T: DeserializeOwned + Default>(&self) -> Result<T, String> {
        let file = self.file;
        match std::fs::read(self.at.join(file)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| format!("{file} is not the record this adapter keeps")),
            Err(why) if why.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(why) => Err(format!("{file} could not be read: {why}")),
        }
    }

    /// Keep `value`, in place of what was kept.
    ///
    /// # Errors
    ///
    /// Why the record could not be written.
    pub(in crate::plex) fn write<T: Serialize>(&self, value: &T) -> Result<(), String> {
        let file = self.file;
        let text = serde_json::to_vec(value).unwrap_or_default();
        let staged = self.at.join(format!("{file}{STAGED}"));
        let _ = std::fs::remove_file(&staged);
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(OWNER_ONLY)
            .open(&staged)
            .and_then(|mut opened| {
                opened.write_all(&text)?;
                opened.sync_all()
            })
            .and_then(|()| std::fs::rename(&staged, self.at.join(file)));
        written.map_err(|why| format!("{file} could not be written: {why}"))
    }
}
