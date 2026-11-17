//! What this adapter says it is, and the kit that serves its contracts.

use std::sync::Arc;

use lemonfiber_adapter_kit::members::Asking;
use lemonfiber_adapter_kit::{Kit, Readiness};
use lemonfiber_contract::adapter::{About, Release};
use lemonfiber_contract::capabilities::{identity, media, owner};

use crate::plex::{presented, Plex, SignIns};

/// Where Plex answers, by its service id and the port it listens on.
pub const ADDRESS: &str = "http://plex:32400";

/// Where the configuration directory Plex runs with is shared into this adapter's
/// container.
pub const SHARED: &str = "/plex";

/// The field of `upstream.json` that holds the code plex.tv showed the operator, which
/// claims Plex.
pub const CODE: &str = "code";

/// The upstream this adapter fronts, as its project names itself.
pub const UPSTREAM: &str = "Plex Media Server";

/// The Plex Media Server release this adapter's recordings were taken from.
pub const RELEASE: &str = "1.43.4.10903-e5521bd8c";

/// The digest of the image that release's recordings were taken from.
pub const DIGEST: &str = "sha256:e0ab27395614a8e1a4fdf84c6bc60ac664915cfdde70c52d030c7728a1c48e14";

/// The variable naming how many sign-ins by a code one device may have waiting at once.
pub const SIGN_INS_PER_DEVICE: &str = "SIGN_INS_PER_DEVICE";

/// The variable naming how many sign-ins by a code may wait at once in all.
pub const SIGN_INS_WAITING: &str = "SIGN_INS_WAITING";

/// How many sign-ins by a code may wait at once, as `per_device` and `in_all` say: the
/// default for whichever says nothing.
///
/// # Errors
///
/// Where either is not a whole number above nought.
pub fn sign_ins(per_device: Option<&str>, in_all: Option<&str>) -> Result<SignIns, String> {
    let defaults = SignIns::default();
    let read = |said: Option<&str>, name: &str, default: usize| {
        said.map_or(Ok(default), |said| {
            said.trim()
                .parse()
                .ok()
                .filter(|count| *count > 0)
                .ok_or_else(|| format!("{name} is {said:?}, not a whole number above nought"))
        })
    };
    Ok(SignIns {
        per_device: read(per_device, SIGN_INS_PER_DEVICE, defaults.per_device)?,
        in_all: read(in_all, SIGN_INS_WAITING, defaults.in_all)?,
    })
}

/// What this adapter says it is: the contracts it speaks and the release it supports.
#[must_use]
pub fn about() -> About {
    About {
        speaks: vec![
            lemonfiber_contract::spoken(owner::claim::CAPABILITY, owner::claim::MAJOR),
            lemonfiber_contract::spoken(media::serve::CAPABILITY, media::serve::MAJOR),
            lemonfiber_contract::spoken(identity::source::CAPABILITY, identity::source::MAJOR),
        ],
        upstream: UPSTREAM.to_owned(),
        releases: vec![Release {
            version: RELEASE.to_owned(),
            digest: DIGEST.to_owned(),
        }],
    }
}

/// The contracts, served from `plex` under `key`, ready while it answers.
#[must_use]
pub fn kit(key: String, plex: Arc<Plex>) -> Kit {
    let asked = Arc::clone(&plex);
    let ready: Readiness = Arc::new(move || {
        let asked = Arc::clone(&asked);
        Box::pin(async move { asked.answers().await })
    });
    let members = Arc::clone(&plex);
    let asking: Asking = Arc::new(move |space, item, token| {
        let members = Arc::clone(&members);
        Box::pin(async move { members.may(&space, &item, &token).await })
    });
    Kit::new(key, about())
        .ready_when(ready)
        .asked_by_members(presented(), asking)
        .serving(owner::claim::served(Arc::clone(&plex)))
        .serving(media::serve::served(Arc::clone(&plex)))
        .serving(identity::source::served(plex))
}

#[cfg(test)]
mod tests;
