//! `identity.source` over Plex. The household is read from plex.tv as the account that
//! claimed Plex (ARCH-R371), and what each member may watch is written in Plex's own terms
//! (ARCH-R378). No member's password passes through this adapter (ARCH-R377), so a sign-in
//! by name and password is unsupported.

use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    Access, Allowed, Certificate, Failure, Household, Invited, MediaServer, Member, NamedLibrary,
    Session, Signed, Unrated,
};
use serde::Deserialize;

use super::keys::{HOME_USERS, SECTIONS, SERVERS, WHO};
use super::{an_id, Plex};

mod accounts;
mod certificates;
mod limits;

use accounts::Account;
use certificates::Rated;

const SHARED_SERVERS: &str = "/api/v2/shared_servers";

const SHARINGS: &str = "/api/v2/sharings";

const RESTRICTED: &str = "/api/v2/home/users/restricted";

const NO_PASSWORDS: &str =
    "a Plex member signs in at plex.tv, so no name and password is checked here";

const NOTHING_ADMITTED: &str = "no certificate of the owner's country is at or under the age limit, so Plex cannot be told to admit only those";

const UNDER_EVERY_PROFILE: &str = "the age limit is under what Plex's youngest restriction profile lets a managed account watch, so no profile holds it";

const NO_SHARE: &str = "the member holds no share of this server to hold to an age limit";

/// Each filter a share holds, as plex.tv names them.
const FILTERS: [&str; 4] = [
    "filterMovies",
    "filterTelevision",
    "filterMusic",
    "filterPhotos",
];

const NO_ADMINISTRATOR: &str =
    "Plex is set up by claiming it, and its owner's password is plex.tv's to change";

#[derive(Deserialize, Default)]
struct Sections {
    #[serde(rename = "Directory", default)]
    directories: Vec<Section>,
}

#[derive(Deserialize)]
struct Section {
    key: String,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize, Default)]
struct Ratings {
    #[serde(rename = "Directory", default)]
    directories: Vec<Rating>,
}

#[derive(Deserialize)]
struct Rating {
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Shared {
    #[serde(default)]
    invited_id: u64,
    #[serde(default)]
    accepted: bool,
}

#[derive(Deserialize)]
struct Owner {
    #[serde(default)]
    country: Option<String>,
}

impl Account {
    fn member(&self) -> Member {
        let (age_limit, unrated) = match (&self.profile, &self.share) {
            (Some(profile), _) if self.managed => (limits::profile_age(profile), Unrated::HeldBack),
            (_, Some(share)) => limits::read(&share.filter),
            _ => (None, Unrated::LetThrough),
        };
        Member {
            id: self.id.clone(),
            name: self.name.clone(),
            claimed: self.accepted,
            access: Access {
                every_library: self.share.as_ref().is_some_and(|share| share.every_library),
                libraries: self
                    .share
                    .as_ref()
                    .map(|share| share.libraries.clone())
                    .unwrap_or_default(),
                age_limit,
                unrated,
                administrator: false,
                disabled: self.closed(),
            },
            last_seen: self
                .share
                .as_ref()
                .and_then(|share| share.last_seen.clone()),
        }
    }
}

/// `said` as a two-letter country code, upper case.
fn a_country(said: &str) -> Option<String> {
    (said.len() == 2 && said.bytes().all(|mark| mark.is_ascii_alphabetic()))
        .then(|| said.to_ascii_uppercase())
}

/// A share's settings holding every filter to `filter`.
fn filtered_by(filter: &str) -> serde_json::Value {
    serde_json::Value::Object(
        FILTERS
            .iter()
            .map(|name| ((*name).to_owned(), serde_json::Value::from(filter)))
            .collect(),
    )
}

impl Plex {
    /// The country of the plex.tv account that claimed Plex.
    async fn owner_country(&self) -> Result<Option<String>, Failure> {
        let response = self.as_owner(Method::Get, WHO, None).await?;
        let owner: Owner = serde_json::from_str(&response.body).map_err(|_| {
            self.accounts
                .refused("plex.tv said which country Plex's owner is in unreadably")
        })?;
        Ok(owner.country.as_deref().and_then(a_country))
    }

    async fn sections(&self) -> Result<Vec<Section>, Failure> {
        let sections: Sections = self
            .read(SECTIONS, "the libraries Plex holds could not be read")
            .await?;
        Ok(sections
            .directories
            .into_iter()
            .filter(|section| an_id(&section.key))
            .collect())
    }

    /// Each certificate Plex's libraries hold that `country`'s table ages.
    async fn held_certificates(&self, country: &str) -> Result<Vec<Rated>, Failure> {
        let mut held: Vec<Rated> = Vec::new();
        for section in self.sections().await? {
            let ratings: Ratings = self
                .read(
                    &format!("{SECTIONS}/{}/contentRating", section.key),
                    "the certificates a library holds could not be read",
                )
                .await?;
            for rating in ratings.directories {
                if let Some(rated) = certificates::rated(country, &rating.title) {
                    if !held.iter().any(|one| one.kept == rated.kept) {
                        held.push(rated);
                    }
                }
            }
        }
        held.sort_by_key(|rated| rated.age);
        Ok(held)
    }

    /// plex.tv's ids for the libraries `libraries`, by the keys Plex files them under,
    /// refused where one is no library of the server `machine`.
    async fn library_ids(&self, machine: &str, libraries: &[String]) -> Result<Vec<u64>, Failure> {
        let at_plex_tv = self.libraries_at_plex_tv(machine).await?;
        libraries
            .iter()
            .map(|key| {
                at_plex_tv
                    .iter()
                    .find(|(held, _)| held == key)
                    .map(|(_, id)| *id)
                    .ok_or_else(|| self.endpoint.refused("no library Plex holds has that key"))
            })
            .collect()
    }

    /// Hold every share with `account` to `filter`, as plex.tv keeps it.
    async fn filter(&self, account: &Account, filter: &str) -> Result<(), Failure> {
        let asked: Vec<(&str, &str)> = FILTERS.iter().map(|name| (*name, filter)).collect();
        self.as_owner(
            Method::Put,
            &format!("{SHARINGS}/{}?{}", account.id, form_encoded(&asked)),
            None,
        )
        .await
        .map(drop)
    }

    /// Open the libraries `ids`, by plex.tv's ids, of the server `machine` to the share `share`.
    async fn shared_libraries(
        &self,
        machine: &str,
        share: u64,
        ids: &[u64],
    ) -> Result<(), Failure> {
        let body = serde_json::json!({
            "server_id": machine,
            "shared_server": { "library_section_ids": ids },
        });
        self.as_owner(
            Method::Put,
            &format!("{SERVERS}/{machine}/shared_servers/{share}"),
            Some(body.to_string()),
        )
        .await
        .map(drop)
    }

    /// The filter `allowed` holds a share to from `kept`, the one it holds now: open, and held
    /// to the limit `allowed` sets or the one `kept` holds.
    fn wanted(&self, kept: &str, allowed: &Allowed) -> Result<String, Failure> {
        if allowed.age_limit.is_none() && allowed.unrated.is_none() {
            return Ok(limits::opening(kept));
        }
        let (held_limit, held_unrated) = limits::read(kept);
        limits::written(
            allowed.age_limit.or(held_limit),
            allowed.unrated.unwrap_or(held_unrated),
        )
        .ok_or_else(|| self.accounts.refused(NOTHING_ADMITTED))
    }

    /// Hold the member `id` to `allowed`. Everything that can be refused is refused before
    /// the first write, the restriction profile is written before any library opens, and
    /// libraries change only while the share admits nothing, so no step that fails leaves
    /// the member able to watch more than before or than `allowed` allows.
    async fn allowing(&self, id: &str, allowed: &Allowed) -> Result<(), Failure> {
        let machine = self.machine().await?;
        let account = self.member_of(&machine, id).await?;
        let limited = allowed.age_limit.is_some() || allowed.unrated.is_some();
        let profile = match (account.managed, allowed.age_limit) {
            (true, Some(age_limit)) => match limits::profile(age_limit) {
                limits::Profiled::Held(profile) => Some(profile),
                limits::Profiled::Unrestricted => Some(""),
                limits::Profiled::Unheld => {
                    return Err(self.accounts.refused(UNDER_EVERY_PROFILE));
                }
            },
            _ => None,
        };
        if !account.managed && limited && account.share.is_none() {
            return Err(self.accounts.refused(NO_SHARE));
        }
        let kept = account
            .share
            .as_ref()
            .map(|share| share.filter.clone())
            .unwrap_or_default();
        let filter = if account.managed {
            limits::opening(&kept)
        } else {
            self.wanted(&kept, allowed)?
        };
        let ids = match &allowed.libraries {
            Some(libraries) => Some(self.library_ids(&machine, libraries).await?),
            None => None,
        };
        if let Some(profile) = profile {
            let asked = form_encoded(&[("restrictionProfile", profile)]);
            self.as_owner(
                Method::Put,
                &format!("{RESTRICTED}/{}?{asked}", account.id),
                None,
            )
            .await?;
        }
        match (&account.share, ids) {
            (None, None) => Ok(()),
            (None, Some(ids)) if ids.is_empty() => Ok(()),
            (None, Some(ids)) => {
                self.share(&machine, &account.id, &ids, filtered_by(&filter))
                    .await
            }
            (Some(_), None) => self.filter(&account, &filter).await,
            (Some(_), Some(ids)) if ids.is_empty() => {
                self.filter(&account, &limits::closing(&filter)).await
            }
            (Some(share), Some(ids)) => {
                self.filter(&account, &limits::closing(&kept)).await?;
                self.shared_libraries(&machine, share.id, &ids).await?;
                self.filter(&account, &filter).await
            }
        }
    }

    async fn unshared(&self, machine: &str, share: u64) -> Result<(), Failure> {
        self.as_owner(
            Method::Delete,
            &format!("{SERVERS}/{machine}/shared_servers/{share}"),
            None,
        )
        .await
        .map(drop)
    }
}

#[async_trait]
impl Household for Plex {
    async fn household(&self) -> Result<Vec<Member>, Failure> {
        let machine = self.machine().await?;
        Ok(self
            .household_of(&machine)
            .await?
            .iter()
            .map(Account::member)
            .collect())
    }

    async fn whoever(
        &self,
        _name: &str,
        _password: &str,
        _device: &str,
    ) -> Result<Option<Signed>, Failure> {
        Err(Self::unsupported(NO_PASSWORDS))
    }

    async fn standing(&self, signed: &Signed) -> Result<bool, Failure> {
        if self.whose(&signed.token).await?.as_deref() != Some(signed.id.as_str()) {
            return Ok(false);
        }
        let machine = self.machine().await?;
        Ok(self
            .household_of(&machine)
            .await?
            .iter()
            .any(|account| account.id == signed.id && !account.closed()))
    }

    async fn invite(&self, name: &str) -> Result<Member, Failure> {
        if name.trim().is_empty() {
            return Err(self
                .accounts
                .refused("an invitation names the plex.tv account it shares Plex with"));
        }
        let machine = self.machine().await?;
        let libraries: Vec<u64> = self
            .libraries_at_plex_tv(&machine)
            .await?
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let body = serde_json::json!({
            "machineIdentifier": machine,
            "invitedEmail": name,
            "librarySectionIds": libraries,
            "settings": filtered_by(limits::CLOSED),
        });
        let response = self
            .as_owner(Method::Post, SHARED_SERVERS, Some(body.to_string()))
            .await?;
        let shared: Shared = serde_json::from_str(&response.body).map_err(|_| {
            self.accounts
                .refused("plex.tv answered the share unreadably")
        })?;
        if shared.invited_id == 0 {
            return Err(self
                .accounts
                .refused("plex.tv shared Plex with no account it names"));
        }
        Ok(Member {
            id: shared.invited_id.to_string(),
            name: name.to_owned(),
            claimed: shared.accepted,
            access: Access {
                every_library: true,
                disabled: true,
                ..Access::default()
            },
            last_seen: None,
        })
    }

    async fn claim(&self, _name: &str, _password: &str, _device: &str) -> Result<bool, Failure> {
        Err(Self::unsupported(NO_PASSWORDS))
    }

    async fn unclaim(&self, id: &str) -> Result<(), Failure> {
        let machine = self.machine().await?;
        let account = self.member_of(&machine, id).await?;
        match (&account.share, account.accepted) {
            (Some(share), false) => self.unshared(&machine, share.id).await,
            _ => Err(self.accounts.refused(
                "the member took the invitation up, so it is withdrawn rather than taken back",
            )),
        }
    }

    async fn withdraw(&self, id: &str) -> Result<(), Failure> {
        let machine = self.machine().await?;
        let account = self.member_of(&machine, id).await?;
        match (&account.share, account.home) {
            (Some(share), false) => self.unshared(&machine, share.id).await,
            _ => self
                .as_owner(Method::Delete, &format!("{HOME_USERS}/{id}"), None)
                .await
                .map(drop),
        }
    }

    async fn when_invited(&self, since: &str) -> Result<Vec<Invited>, Failure> {
        let machine = self.machine().await?;
        Ok(self
            .household_of(&machine)
            .await?
            .into_iter()
            .filter_map(|account| {
                let at = account.linked_at?;
                (at.as_str() >= since).then_some(Invited {
                    member: account.id,
                    at,
                })
            })
            .collect())
    }

    async fn libraries(&self) -> Result<Vec<NamedLibrary>, Failure> {
        Ok(self
            .sections()
            .await?
            .into_iter()
            .map(|section| NamedLibrary {
                id: section.key,
                name: section.title,
            })
            .collect())
    }

    async fn ratings(&self) -> Result<Vec<Certificate>, Failure> {
        let Some(country) = self
            .owner_country()
            .await?
            .filter(|country| certificates::holds(country))
        else {
            return Ok(Vec::new());
        };
        let mut named: Vec<Certificate> = Vec::new();
        for rated in self.held_certificates(&country).await? {
            if !named.iter().any(|one| one.name == rated.name) {
                named.push(Certificate {
                    name: rated.name,
                    age: rated.age,
                });
            }
        }
        Ok(named)
    }

    async fn country(&self) -> Result<Option<String>, Failure> {
        self.owner_country().await
    }

    async fn allow(&self, id: &str, allowed: &Allowed) -> Result<(), Failure> {
        self.allowing(id, allowed).await
    }

    async fn claimable(&self, id: &str, allowed: &Allowed) -> Result<(), Failure> {
        self.allowing(id, allowed).await
    }

    async fn suspend(&self, id: &str) -> Result<(), Failure> {
        let machine = self.machine().await?;
        let account = self.member_of(&machine, id).await?;
        match &account.share {
            Some(share) => self.filter(&account, &limits::closing(&share.filter)).await,
            None => Ok(()),
        }
    }

    async fn sessions(&self, member: &str) -> Result<Vec<Session>, Failure> {
        if !an_id(member) {
            return Ok(Vec::new());
        }
        self.playing_for(member).await
    }

    async fn signs_devices_in(&self) -> Result<bool, Failure> {
        Ok(true)
    }
}

#[async_trait]
impl MediaServer for Plex {
    async fn startup_completed(&self) -> Result<bool, Failure> {
        Ok(self.token().await?.is_some())
    }

    async fn create_admin(&self, _name: &str, _password: &str) -> Result<(), Failure> {
        Err(Self::unsupported(NO_ADMINISTRATOR))
    }

    async fn replace_admin_password(&self, _replacement: &str) -> Result<(), Failure> {
        Err(Self::unsupported(NO_ADMINISTRATOR))
    }
}

#[cfg(test)]
mod tests;
