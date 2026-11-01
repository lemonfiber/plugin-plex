//! The Home accounts this adapter made as apps' keys, by app, kept in its own
//! configuration directory so that nothing but an account it made is ever listed, switched
//! to or removed as a key, whatever else is titled for an app.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The file the accounts are kept in, in the adapter's configuration directory.
const KEPT_FILE: &str = "plex-app-accounts.json";

const STAGED: &str = "plex-app-accounts.json.staged";

const OWNER_ONLY: u32 = 0o600;

/// The ids of the accounts made for each app.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Made {
    apps: BTreeMap<String, BTreeSet<String>>,
}

impl Made {
    /// The ids of the accounts made for `app`.
    pub(super) fn of(&self, app: &str) -> BTreeSet<String> {
        self.apps.get(app).cloned().unwrap_or_default()
    }

    /// Whether the account `id` was made for any app.
    pub(super) fn holds(&self, id: &str) -> bool {
        self.apps.values().any(|ids| ids.contains(id))
    }
}

/// Where the accounts are kept.
#[derive(Debug, Clone)]
pub(in crate::plex) struct Kept {
    at: PathBuf,
}

impl Kept {
    /// The accounts kept in the configuration directory `directory`.
    pub(in crate::plex) fn in_directory(directory: &Path) -> Self {
        Self {
            at: directory.to_path_buf(),
        }
    }

    /// The accounts made so far: none where nothing was ever kept.
    ///
    /// # Errors
    ///
    /// Why the record could not be read, where it is there and unreadable.
    pub(super) fn read(&self) -> Result<Made, String> {
        match std::fs::read(self.at.join(KEPT_FILE)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| format!("{KEPT_FILE} is not the record this adapter keeps")),
            Err(why) if why.kind() == std::io::ErrorKind::NotFound => Ok(Made::default()),
            Err(why) => Err(format!("{KEPT_FILE} could not be read: {why}")),
        }
    }

    /// Record `id` as made for `app`.
    ///
    /// # Errors
    ///
    /// Why the record could not be read or written.
    pub(super) fn made(&self, app: &str, id: &str) -> Result<(), String> {
        let mut made = self.read()?;
        made.apps
            .entry(app.to_owned())
            .or_default()
            .insert(id.to_owned());
        self.write(&made)
    }

    /// Record `id` as made for no app any more.
    ///
    /// # Errors
    ///
    /// Why the record could not be read or written.
    pub(super) fn gone(&self, id: &str) -> Result<(), String> {
        let mut made = self.read()?;
        for ids in made.apps.values_mut() {
            ids.remove(id);
        }
        made.apps.retain(|_, ids| !ids.is_empty());
        self.write(&made)
    }

    fn write(&self, made: &Made) -> Result<(), String> {
        let text = serde_json::to_vec(made).map_err(|why| why.to_string())?;
        let staged = self.at.join(STAGED);
        let _ = std::fs::remove_file(&staged);
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(OWNER_ONLY)
            .open(&staged)
            .and_then(|mut file| {
                file.write_all(&text)?;
                file.sync_all()
            })
            .and_then(|()| std::fs::rename(&staged, self.at.join(KEPT_FILE)));
        written.map_err(|why| format!("{KEPT_FILE} could not be written: {why}"))
    }
}

#[cfg(test)]
mod tests;
