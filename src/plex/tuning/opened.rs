//! The live sessions this adapter tuned and has not closed, kept in its own configuration
//! directory so that it ends no session it did not open.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::plex::record::Record;

/// The file the sessions are kept in, in the adapter's configuration directory.
const OPENED_FILE: &str = "plex-live-sessions.json";

/// The ids of the live sessions tuned and not yet closed.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sessions {
    opened: BTreeSet<String>,
}

/// Where the live sessions are kept.
#[derive(Debug, Clone)]
pub(in crate::plex) struct Opened {
    record: Record,
    changing: Arc<Mutex<()>>,
}

impl Opened {
    /// The live sessions kept in the configuration directory `directory`.
    pub(in crate::plex) fn in_directory(directory: &Path) -> Self {
        Self {
            record: Record::in_directory(directory, OPENED_FILE),
            changing: Arc::default(),
        }
    }

    /// Whether `id` is a live session this adapter tuned and has not closed.
    ///
    /// # Errors
    ///
    /// Why the record could not be read.
    pub(super) fn holds(&self, id: &str) -> Result<bool, String> {
        self.record
            .read::<Sessions>()
            .map(|sessions| sessions.opened.contains(id))
    }

    /// Record `id` as tuned.
    ///
    /// # Errors
    ///
    /// Why the record could not be read or written.
    pub(super) fn tuned(&self, id: &str) -> Result<(), String> {
        self.changed(|opened| {
            opened.insert(id.to_owned());
        })
    }

    /// Record `id` as closed.
    ///
    /// # Errors
    ///
    /// Why the record could not be read or written.
    pub(super) fn closed(&self, id: &str) -> Result<(), String> {
        self.changed(|opened| {
            opened.remove(id);
        })
    }

    fn changed(&self, change: impl FnOnce(&mut BTreeSet<String>)) -> Result<(), String> {
        let _held = self
            .changing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut sessions = self.record.read::<Sessions>()?;
        change(&mut sessions.opened);
        self.record.write(&sessions)
    }
}
