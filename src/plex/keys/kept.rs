//! The Home accounts this adapter made as apps' keys, by app, kept in its own
//! configuration directory so that nothing but an account it made is ever listed, switched
//! to or removed as a key, whatever else is titled for an app.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::plex::record::Record;

/// The file the accounts are kept in, in the adapter's configuration directory.
const KEPT_FILE: &str = "plex-app-accounts.json";

/// The ids of the accounts made for each app.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::plex) struct Made {
    apps: BTreeMap<String, BTreeSet<String>>,
}

impl Made {
    /// The ids of the accounts made for `app`.
    pub(super) fn of(&self, app: &str) -> BTreeSet<String> {
        self.apps.get(app).cloned().unwrap_or_default()
    }

    /// Whether the account `id` was made for any app.
    pub(in crate::plex) fn holds(&self, id: &str) -> bool {
        self.apps.values().any(|ids| ids.contains(id))
    }
}

/// Where the accounts are kept.
#[derive(Debug, Clone)]
pub(in crate::plex) struct Kept {
    record: Record,
}

impl Kept {
    /// The accounts kept in the configuration directory `directory`.
    pub(in crate::plex) fn in_directory(directory: &Path) -> Self {
        Self {
            record: Record::in_directory(directory, KEPT_FILE),
        }
    }

    /// The accounts made so far: none where nothing was ever kept.
    ///
    /// # Errors
    ///
    /// Why the record could not be read, where it is there and unreadable.
    pub(in crate::plex) fn read(&self) -> Result<Made, String> {
        self.record.read()
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
        self.record.write(made)
    }
}

#[cfg(test)]
mod tests;
