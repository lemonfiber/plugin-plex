//! Whom a member watches live TV as on Plex: their own server token, a share's as plex.tv
//! lists it on this server or a Home account's as switching to it hands it, and what their
//! share's television filter and restriction profile keep from them. A member may watch
//! live TV only where their share lets them use the server's tuners.

use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::Failure;

use super::super::keys::{attributes, SERVERS};
use super::super::Plex;
use super::accounts::Account;
use super::limits;

const SHARED: &str = "SharedServer";

/// A member who may watch live TV on this server, and what is kept from them.
pub(in crate::plex) struct Watcher {
    /// Their own server token.
    pub(in crate::plex) token: String,
    television: String,
    profile: Option<String>,
}

impl Watcher {
    /// Whether anything is kept from them: a television filter or a restriction profile.
    pub(in crate::plex) fn restricted(&self) -> bool {
        !self.television.is_empty() || self.profile.is_some()
    }

    /// Whether a programme rated `certificate` and carrying `labels` is one their share's
    /// television filter and their restriction profile both let them watch.
    pub(in crate::plex) fn admits(&self, certificate: Option<&str>, labels: &[String]) -> bool {
        limits::admits(&self.television, certificate, labels)
            && self
                .profile
                .as_deref()
                .is_none_or(|profile| limits::profile_admits(profile, certificate))
    }
}

impl Plex {
    /// The member `member` as they watch live TV on this server, or none where they are no
    /// member of its household, their share does not let them use its tuners or admits
    /// nothing, or no token of their own is found.
    pub(in crate::plex) async fn watcher(&self, member: &str) -> Result<Option<Watcher>, Failure> {
        let machine = self.machine().await?;
        let Some(account) = self
            .household_of(&machine)
            .await?
            .into_iter()
            .find(|account| account.id == member)
        else {
            return Ok(None);
        };
        let Some(share) = account
            .share
            .as_ref()
            .filter(|share| share.tuners && !limits::closed(&share.television))
        else {
            return Ok(None);
        };
        let Some(token) = self.own_token(&machine, &account).await? else {
            return Ok(None);
        };
        Ok(Some(Watcher {
            token,
            television: share.television.clone(),
            profile: account.profile.clone().filter(|_| account.managed),
        }))
    }

    async fn own_token(&self, machine: &str, account: &Account) -> Result<Option<String>, Failure> {
        let response = self
            .as_owner(
                Method::Get,
                &format!("{SERVERS}/{machine}/shared_servers"),
                None,
            )
            .await?;
        let shared = attributes(&response.body, SHARED, &["userID", "accessToken"])
            .into_iter()
            .find_map(|read| match read.as_slice() {
                [Some(user), Some(token)] if *user == account.id && !token.is_empty() => {
                    Some(token.clone())
                }
                _ => None,
            });
        match shared {
            Some(token) => Ok(Some(token)),
            None if account.home => self.switched_to(&account.id).await.map(Some),
            None => Ok(None),
        }
    }
}
