//! A member signing in by a code plex.tv issues: a PIN the member approves at
//! `plex.tv/link`, asked about until plex.tv hands over the token it proved, so no password
//! passes through lemonfiber or this adapter.
//!
//! The id the core is handed names nothing of the PIN or the device. It is drawn from the
//! operating system's randomness, and only this adapter knows which PIN it stands for, until
//! the PIN lapses or proves somebody once.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use lemonfiber_ports::http::{Method, Request, Response, JSON};
use lemonfiber_ports::random::Random;
use lemonfiber_ports::service::{Failure, SignInCode, Signed};
use lemonfiber_ports::Instant;
use serde::Deserialize;

use super::super::Plex;

/// Where plex.tv issues a PIN and answers what became of one.
const PINS: &str = "/api/v2/pins";

/// Where a member approves a PIN.
const APPROVE_AT: &str = "https://plex.tv/link";

/// What plex.tv is told is asking for the PIN.
const PRODUCT: (&str, &str) = ("X-Plex-Product", "lemonfiber");

/// The header naming the device a PIN is issued to and asked about from.
const CLIENT: &str = "X-Plex-Client-Identifier";

/// The status plex.tv answers a PIN it no longer holds with.
const GONE: u16 = 404;

/// Said where no id could be drawn for a sign-in.
const UNDRAWN: &str = "no id could be drawn for a sign-in by a code";

/// Said where the sign-ins waiting could not be read.
const UNREAD: &str = "the sign-ins by a code waiting could not be read";

/// Said where as many sign-ins wait as may.
const FULL: &str = "no more sign-ins by a code may wait now";

/// How many random bytes an id is drawn from.
const ID_BYTES: usize = 16;

/// How many sign-ins may wait at once: for one device, and in all. A device asking past
/// its share lets go of its own oldest; a full table refuses a new one until one lapses or
/// is spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignIns {
    /// For one device.
    pub per_device: usize,
    /// In all.
    pub in_all: usize,
}

impl Default for SignIns {
    fn default() -> Self {
        Self {
            per_device: 1,
            in_all: 64,
        }
    }
}

/// A PIN as plex.tv answers one.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pin {
    id: u64,
    code: String,
    expires_at: String,
    auth_token: Option<String>,
}

/// A PIN issued and not yet spent: plex.tv's id for it, the device it was issued to, when
/// it lapses, which is when plex.tv says the PIN does, and its place in the order issued.
#[derive(Clone)]
struct Waiting {
    pin: u64,
    device: String,
    lapses: Instant,
    issued: u64,
}

/// Every sign-in waiting, by the id the core holds, and the place the next one issued takes.
#[derive(Default)]
struct Held {
    waiting: HashMap<String, Waiting>,
    next: u64,
}

impl Held {
    /// How many of `device`'s own sign-ins wait past its share under `limits`, keeping room
    /// for one more.
    fn past_share(&self, device: &str, limits: SignIns) -> usize {
        self.waiting
            .values()
            .filter(|one| one.device == device)
            .count()
            .saturating_sub(limits.per_device.saturating_sub(1))
    }

    /// Whether one more sign-in for `device` fits under `limits`, once its own past its
    /// share are let go.
    fn fits(&self, device: &str, limits: SignIns) -> bool {
        self.waiting.len() - self.past_share(device, limits) < limits.in_all
    }

    /// Make room for one more sign-in for `device` under `limits` by letting go of its own
    /// oldest past its share, and say whether it fits; nothing is let go where it does not.
    fn room_for(&mut self, device: &str, limits: SignIns) -> bool {
        if !self.fits(device, limits) {
            return false;
        }
        for _ in 0..self.past_share(device, limits) {
            let oldest = self
                .waiting
                .iter()
                .filter(|(_, one)| one.device == device)
                .min_by_key(|(_, one)| one.issued)
                .map(|(id, _)| id.clone());
            if let Some(id) = oldest {
                self.waiting.remove(&id);
            }
        }
        true
    }
}

/// The sign-ins waiting on a PIN, how many may wait, and the randomness each id is drawn
/// from.
pub(in crate::plex) struct Signing {
    held: Arc<Mutex<Held>>,
    limits: SignIns,
    random: Arc<dyn Random>,
}

impl Default for Signing {
    fn default() -> Self {
        Self::under(SignIns::default(), Arc::new(lemonfiber_random::Os))
    }
}

impl Signing {
    /// No sign-in waiting yet, as many waiting at once as `limits` lets, each id drawn
    /// from `random`.
    pub(in crate::plex) fn under(limits: SignIns, random: Arc<dyn Random>) -> Self {
        Self {
            held: Arc::default(),
            limits,
            random,
        }
    }

    /// Whether one more sign-in for `device` fits at `now`, asked before plex.tv is.
    ///
    /// # Errors
    ///
    /// Why it does not: the sign-ins waiting could not be read, or the table is full.
    pub(in crate::plex) fn room(&self, device: &str, now: Instant) -> Result<(), &'static str> {
        let mut held = self.held.lock().map_err(|_| UNREAD)?;
        held.waiting.retain(|_, one| one.lapses > now);
        if held.fits(device, self.limits) {
            Ok(())
        } else {
            Err(FULL)
        }
    }

    /// Keep the PIN `pin` issued to `device`, lapsing at `lapses`, under a fresh id, after
    /// letting go of every sign-in lapsed at `now` and making room for it.
    ///
    /// # Errors
    ///
    /// Why it was not kept: no id could be drawn, the sign-ins waiting could not be read, or
    /// the table is full.
    pub(in crate::plex) fn kept(
        &self,
        (pin, device, lapses): (u64, &str, Instant),
        now: Instant,
    ) -> Result<String, &'static str> {
        let drawn = self.random.bytes(ID_BYTES).ok_or(UNDRAWN)?;
        let id = drawn
            .iter()
            .fold(String::with_capacity(ID_BYTES * 2), |mut id, byte| {
                let _ = write!(id, "{byte:02x}");
                id
            });
        let mut held = self.held.lock().map_err(|_| UNREAD)?;
        held.waiting.retain(|_, one| one.lapses > now);
        if !held.room_for(device, self.limits) {
            return Err(FULL);
        }
        held.next = held.next.saturating_add(1);
        let waiting = Waiting {
            pin,
            device: device.to_owned(),
            lapses,
            issued: held.next,
        };
        held.waiting.insert(id.clone(), waiting);
        Ok(id)
    }

    /// The sign-in `id` names, where it waits and has not lapsed at `now`.
    fn waiting(&self, id: &str, now: Instant) -> Option<Waiting> {
        let mut held = self.held.lock().ok()?;
        held.waiting.retain(|_, one| one.lapses > now);
        held.waiting.get(id).cloned()
    }

    /// Let go of the sign-in `id` names.
    fn spent(&self, id: &str) {
        if let Ok(mut held) = self.held.lock() {
            held.waiting.remove(id);
        }
    }
}

/// The moment it is now.
fn now() -> Instant {
    Instant::of(SystemTime::now())
}

impl Plex {
    /// A PIN plex.tv issues for `device`, kept under a fresh id, with where the member
    /// approves it.
    pub(super) async fn pin_for(&self, device: &str) -> Result<SignInCode, Failure> {
        if device.is_empty() {
            return Err(self
                .accounts
                .refused("plex.tv is not asked for a PIN for no device"));
        }
        self.signing
            .room(device, now())
            .map_err(|why| self.accounts.refused(why))?;
        let response = self.pins(Method::Post, PINS, device).await?;
        let pin = self.pin(&response)?;
        let lapses_at = Instant::from_service_text(&pin.expires_at).ok_or_else(|| {
            self.accounts
                .refused("plex.tv said when the PIN lapses unreadably")
        })?;
        let id = self
            .signing
            .kept((pin.id, device, lapses_at), now())
            .map_err(|why| self.accounts.refused(why))?;
        Ok(SignInCode {
            id,
            code: pin.code,
            approve_at: APPROVE_AT.to_owned(),
            lapses_at,
        })
    }

    /// The member the sign-in `id` names proved, once plex.tv holds a token for its PIN and
    /// the account is one of the household's; nothing before then, after it lapses, for an
    /// id this adapter did not issue, or for one already spent.
    pub(super) async fn pin_proved(&self, id: &str) -> Result<Option<Signed>, Failure> {
        let Some(waiting) = self.signing.waiting(id, now()) else {
            return Ok(None);
        };
        let response = self
            .pins(
                Method::Get,
                &format!("{PINS}/{}", waiting.pin),
                &waiting.device,
            )
            .await?;
        if response.status == GONE {
            self.signing.spent(id);
            return Ok(None);
        }
        let Some(token) = self.pin(&response)?.auth_token else {
            return Ok(None);
        };
        self.signing.spent(id);
        let Some(account) = self.whose(&token).await? else {
            return Ok(None);
        };
        let signed = Signed { id: account, token };
        Ok(self.standing_of(&signed).await?.then_some(signed))
    }

    /// plex.tv's answer to `method` at `path` about a PIN for `device`.
    async fn pins(&self, method: Method, path: &str, device: &str) -> Result<Response, Failure> {
        self.accounts
            .send(&Request {
                method,
                url: self.accounts.url(path),
                headers: vec![
                    (PRODUCT.0.to_owned(), PRODUCT.1.to_owned()),
                    (CLIENT.to_owned(), device.to_owned()),
                    ("Accept".to_owned(), JSON.to_owned()),
                ],
                body: None,
                pinned: None,
                deadline: None,
            })
            .await
    }

    /// The PIN `response` answers, refused where it is not a success or cannot be read.
    fn pin(&self, response: &Response) -> Result<Pin, Failure> {
        if !response.is_success() {
            return Err(self.accounts.refusal(response));
        }
        serde_json::from_str(&response.body)
            .map_err(|_| self.accounts.refused("plex.tv answered the PIN unreadably"))
    }
}
