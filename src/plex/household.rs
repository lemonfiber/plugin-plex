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
    fn member(&self, country: Option<&str>) -> Member {
        let (age_limit, unrated) = match (&self.profile, &self.share) {
            (Some(profile), _) if self.managed => (limits::profile_age(profile), Unrated::HeldBack),
            (_, Some(share)) => country.map_or((None, Unrated::LetThrough), |country| {
                limits::read(country, &share.filter)
            }),
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

    /// Open the libraries `libraries`, by the keys Plex files them under, to `account` on
    /// the server `machine`.
    async fn open(
        &self,
        machine: &str,
        account: &Account,
        libraries: &[String],
    ) -> Result<(), Failure> {
        let at_plex_tv = self.libraries_at_plex_tv(machine).await?;
        let mut ids = Vec::new();
        for key in libraries {
            let Some((_, id)) = at_plex_tv.iter().find(|(held, _)| held == key) else {
                return Err(self.endpoint.refused("no library Plex holds has that key"));
            };
            ids.push(*id);
        }
        let Some(share) = &account.share else {
            return self.share(machine, &account.id, &ids).await;
        };
        let body = serde_json::json!({
            "server_id": machine,
            "shared_server": { "library_section_ids": ids },
        });
        self.as_owner(
            Method::Put,
            &format!("{SERVERS}/{machine}/shared_servers/{}", share.id),
            Some(body.to_string()),
        )
        .await
        .map(drop)
    }

    /// Hold `account` to the age limit and the choice about unrated content in `allowed`,
    /// keeping what `allowed` leaves unsaid as the account has it.
    async fn hold(&self, account: &Account, allowed: &Allowed) -> Result<(), Failure> {
        if account.managed {
            let Some(age_limit) = allowed.age_limit else {
                return Ok(());
            };
            let asked = form_encoded(&[(
                "restrictionProfile",
                limits::profile(age_limit).unwrap_or_default(),
            )]);
            return self
                .as_owner(
                    Method::Put,
                    &format!("{RESTRICTED}/{}?{asked}", account.id),
                    None,
                )
                .await
                .map(drop);
        }
        let Some(share) = &account.share else {
            return Err(self
                .accounts
                .refused("the member holds no share of this server to limit"));
        };
        let country = self
            .owner_country()
            .await?
            .filter(|country| certificates::holds(country))
            .ok_or_else(|| {
                self.accounts.refused(
                    "this adapter ages no certificates of the country plex.tv names for Plex's \
                     owner, so an age limit cannot be said in them",
                )
            })?;
        let (held_limit, held_unrated) = limits::read(&country, &share.filter);
        let Some(age_limit) = allowed.age_limit.or(held_limit) else {
            return Ok(());
        };
        let unrated = allowed.unrated.unwrap_or(held_unrated);
        let filter = limits::written(age_limit, unrated, &certificates::every(&country))
            .ok_or_else(|| self.accounts.refused(NOTHING_ADMITTED))?;
        let asked = form_encoded(&[
            ("filterMovies", filter.as_str()),
            ("filterTelevision", filter.as_str()),
        ]);
        self.as_owner(
            Method::Put,
            &format!("{SHARINGS}/{}?{asked}", account.id),
            None,
        )
        .await
        .map(drop)
    }

    async fn allowing(&self, id: &str, allowed: &Allowed) -> Result<(), Failure> {
        let machine = self.machine().await?;
        let account = self.member_of(&machine, id).await?;
        if let Some(libraries) = &allowed.libraries {
            self.open(&machine, &account, libraries).await?;
        }
        if allowed.age_limit.is_some() || allowed.unrated.is_some() {
            let account = self.member_of(&machine, id).await?;
            self.hold(&account, allowed).await?;
        }
        Ok(())
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
        let household = self.household_of(&machine).await?;
        let country = self.owner_country().await?;
        Ok(household
            .iter()
            .map(|account| account.member(country.as_deref()))
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
            "settings": {},
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
        if account.share.is_none() {
            return Ok(());
        }
        self.open(&machine, &account, &[]).await
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
