//! The household over Plex (ARCH-R371): the Home accounts of the account that claimed Plex
//! and the plex.tv accounts this server is shared with, never an account this adapter made
//! as an app's key, the request service's among them.

use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::Failure;
use serde::Deserialize;

use super::super::keys::{attributes, FLAGGED, HOME_USERS, TITLED};
use super::super::Plex;

const FRIENDS: &str = "/api/v2/friends";

/// How plex.tv names a share somebody took up, and one still waiting for them.
const STANDINGS: [&str; 2] = ["accepted", "pending_sent"];

const ACCEPTED: &str = "accepted";

const ACCOUNT: &str = "User";

/// One share of a server, as plex.tv lists it on the account it is shared with.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedServer {
    id: u64,
    #[serde(default)]
    machine_identifier: String,
    #[serde(default)]
    all_libraries: bool,
    #[serde(default)]
    libraries: Vec<SharedLibrary>,
    #[serde(default)]
    last_seen_at: Option<String>,
    #[serde(default)]
    sharing_settings: Option<Settings>,
}

#[derive(Debug, Clone, Deserialize)]
struct SharedLibrary {
    key: serde_json::Value,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    #[serde(default)]
    filter_movies: Option<String>,
}

/// One plex.tv account the claiming account is linked to, as plex.tv lists it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Friend {
    id: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    restricted: bool,
    #[serde(default)]
    home: bool,
    #[serde(default)]
    restriction_profile: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    friendship_created_at: Option<String>,
    #[serde(default)]
    sharing_settings: Option<Settings>,
    #[serde(default)]
    shared_servers: Vec<SharedServer>,
}

/// One share of this server with a member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Share {
    /// The id plex.tv files the share under.
    pub(super) id: u64,
    /// Whether it opens every library.
    pub(super) every_library: bool,
    /// The keys Plex files the libraries it opens under, where not every one.
    pub(super) libraries: Vec<String>,
    /// The filter of certificates it holds the member to, as plex.tv keeps it.
    pub(super) filter: String,
    /// When plex.tv last saw the member on this server.
    pub(super) last_seen: Option<String>,
}

impl Share {
    /// Whether it opens nothing: no library, or a filter admitting nothing.
    pub(super) fn closed(&self) -> bool {
        (!self.every_library && self.libraries.is_empty()) || super::limits::closed(&self.filter)
    }
}

/// One member of the household.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Account {
    /// The id plex.tv files the account under.
    pub(super) id: String,
    /// What the account is called.
    pub(super) name: String,
    /// Whether it is an account of the claiming account's Home.
    pub(super) home: bool,
    /// Whether it is a managed account of that Home, with no plex.tv sign-in of its own.
    pub(super) managed: bool,
    /// The restriction profile Plex holds a managed account to.
    pub(super) profile: Option<String>,
    /// Whether the member took the share up, which a Home account has.
    pub(super) accepted: bool,
    /// The share of this server with it.
    pub(super) share: Option<Share>,
    /// When plex.tv linked the account to the claiming account.
    pub(super) linked_at: Option<String>,
}

impl Account {
    /// Whether it may watch nothing on this server.
    pub(super) fn closed(&self) -> bool {
        self.share.as_ref().is_none_or(Share::closed)
    }
}

/// One account of the claiming account's Home, as `/api/home/users` lists it.
struct HomeAccount {
    id: String,
    title: String,
    managed: bool,
    administrator: bool,
    profile: Option<String>,
}

fn home_accounts(xml: &str) -> Vec<HomeAccount> {
    attributes(
        xml,
        ACCOUNT,
        &["id", "title", "restricted", "admin", "restrictionProfile"],
    )
    .into_iter()
    .filter_map(|read| match read.as_slice() {
        [Some(id), Some(title), restricted, admin, profile] if super::super::an_id(id) => {
            Some(HomeAccount {
                id: id.clone(),
                title: title.clone(),
                managed: restricted.as_deref() == Some(FLAGGED),
                administrator: admin.as_deref() == Some(FLAGGED),
                profile: profile.clone().filter(|named| !named.is_empty()),
            })
        }
        _ => None,
    })
    .collect()
}

fn key_of(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(key) => Some(key.clone()),
        serde_json::Value::Number(key) => Some(key.to_string()),
        _ => None,
    }
}

fn share_of(friend: &Friend, machine: &str) -> Option<Share> {
    let shared = friend
        .shared_servers
        .iter()
        .find(|shared| shared.machine_identifier == machine)?;
    let filter = shared
        .sharing_settings
        .as_ref()
        .or(friend.sharing_settings.as_ref())
        .and_then(|settings| settings.filter_movies.clone())
        .unwrap_or_default();
    Some(Share {
        id: shared.id,
        every_library: shared.all_libraries,
        libraries: shared
            .libraries
            .iter()
            .filter_map(|library| key_of(&library.key))
            .collect(),
        filter,
        last_seen: shared.last_seen_at.clone(),
    })
}

impl Plex {
    async fn linked(&self, standing: &str) -> Result<Vec<Friend>, Failure> {
        let asked = form_encoded(&[
            ("includeSharedServers", "1"),
            ("includeSharingSettings", "1"),
            ("status", standing),
        ]);
        let response = self
            .as_owner(Method::Get, &format!("{FRIENDS}?{asked}"), None)
            .await?;
        serde_json::from_str(&response.body).map_err(|_| {
            self.accounts
                .refused("plex.tv listed the accounts linked to Plex's owner unreadably")
        })
    }

    /// Every member of the household this server `machine` serves.
    pub(super) async fn household_of(&self, machine: &str) -> Result<Vec<Account>, Failure> {
        let made = self.kept()?.read().map_err(|why| self.unkept(&why))?;
        let response = self.as_owner(Method::Get, HOME_USERS, None).await?;
        let home = home_accounts(&response.body);
        let ours = |id: &str, title: &str, managed: bool| {
            made.holds(id) || (managed && title.starts_with(TITLED))
        };
        let mut household: Vec<Account> = home
            .iter()
            .filter(|account| {
                !account.administrator && !ours(&account.id, &account.title, account.managed)
            })
            .map(|account| Account {
                id: account.id.clone(),
                name: account.title.clone(),
                home: true,
                managed: account.managed,
                profile: account.profile.clone(),
                accepted: true,
                share: None,
                linked_at: None,
            })
            .collect();
        for standing in STANDINGS {
            for friend in self.linked(standing).await? {
                let id = friend.id.to_string();
                if ours(&id, &friend.title, friend.restricted) {
                    continue;
                }
                let share = share_of(&friend, machine);
                if let Some(account) = household.iter_mut().find(|account| account.id == id) {
                    account.share = account.share.take().or(share);
                    account.linked_at = account
                        .linked_at
                        .take()
                        .or_else(|| friend.friendship_created_at.clone());
                    continue;
                }
                if share.is_none() {
                    continue;
                }
                household.push(Account {
                    id,
                    name: friend.title.clone(),
                    home: friend.home,
                    managed: friend.restricted,
                    profile: friend.restriction_profile.clone(),
                    accepted: friend.status.as_deref() == Some(ACCEPTED),
                    share,
                    linked_at: friend.friendship_created_at.clone(),
                });
            }
        }
        Ok(household)
    }

    /// The member `id` of the household this server `machine` serves, refused where there
    /// is none.
    pub(super) async fn member_of(&self, machine: &str, id: &str) -> Result<Account, Failure> {
        self.household_of(machine)
            .await?
            .into_iter()
            .find(|account| account.id == id)
            .ok_or_else(|| {
                self.accounts
                    .refused("no member of the household Plex serves has that id")
            })
    }
}
