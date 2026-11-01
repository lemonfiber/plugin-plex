//! A key for an app on Plex: a managed account in the Plex Home of the account that claimed
//! Plex, titled for the app and shared every library, whose token is the key (ARCH-R370).
//! Plex keeps no key of its own, so every step is asked of plex.tv with the token Plex keeps
//! for the claiming account, and the adapter holds nothing between calls.

use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::{Method, Request, Response};
use lemonfiber_ports::service::{AppKeys, Dated, Failure};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use serde::Deserialize;

use super::{as_a_server, preferences, Plex, IDENTITY};

const HOME_USERS: &str = "/api/home/users";

const WHO: &str = "/api/v2/user";

const SECTIONS: &str = "/library/sections";

/// What every account an app's key is starts its title with.
const TITLED: &str = "lemonfiber-";

const ACCOUNT: &str = "User";

const SWITCHED: &str = "authenticationToken";

const JSON: &str = "application/json";

const REFUSED_THE_TOKEN: u16 = 401;

/// The longest app name a title is made from.
const LONGEST_APP: usize = 40;

/// One account of the claiming account's Plex Home.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HomeUser {
    id: String,
    title: String,
}

#[derive(Deserialize)]
struct Who {
    id: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Served {
    machine_identifier: String,
}

#[derive(Deserialize, Default)]
struct Sections {
    #[serde(rename = "Directory", default)]
    directories: Vec<Section>,
}

#[derive(Deserialize)]
struct Section {
    key: String,
}

/// The title of the account an app's key is, where `app` is one lowercase word of letters,
/// digits and `-`.
fn titled(app: &str) -> Option<String> {
    (!app.is_empty()
        && app.len() <= LONGEST_APP
        && app
            .bytes()
            .all(|mark| mark.is_ascii_lowercase() || mark.is_ascii_digit() || mark == b'-'))
    .then(|| format!("{TITLED}{app}"))
}

/// The values of each of `names` on every element named `element` in `xml`, in order.
fn attributes(xml: &str, element: &str, names: &[&str]) -> Vec<Vec<Option<String>>> {
    let mut reader = Reader::from_str(xml);
    let mut found = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag) | Event::Empty(tag)) if tag.name().as_ref() == element => {
                let read: Vec<(String, String)> = tag
                    .attributes()
                    .filter_map(Result::ok)
                    .filter_map(|attribute| {
                        let value = attribute.normalized_value(XmlVersion::default()).ok()?;
                        Some((attribute.key.as_ref().to_owned(), value.into_owned()))
                    })
                    .collect();
                found.push(
                    names
                        .iter()
                        .map(|name| {
                            read.iter()
                                .find(|(key, _)| key == name)
                                .map(|(_, value)| value.clone())
                        })
                        .collect(),
                );
            }
            Ok(Event::Eof) | Err(_) => return found,
            Ok(_) => {}
        }
    }
}

/// The accounts of the Plex Home `xml` lists.
fn home_users(xml: &str) -> Vec<HomeUser> {
    attributes(xml, ACCOUNT, &["id", "title"])
        .into_iter()
        .filter_map(|read| match read.as_slice() {
            [Some(id), Some(title)] if super::an_id(id) => Some(HomeUser {
                id: id.clone(),
                title: title.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// The token `xml`, plex.tv's answer to switching to an account, names.
fn switched_token(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag) | Event::Empty(tag)) => {
                return tag
                    .attributes()
                    .filter_map(Result::ok)
                    .find(|attribute| attribute.key.as_ref() == SWITCHED)
                    .and_then(|attribute| attribute.normalized_value(XmlVersion::default()).ok())
                    .map(|value| value.trim().to_owned())
                    .filter(|token| lemonfiber_adapter_kit::credential::spelled_as_a_key(token));
            }
            Ok(Event::Eof) | Err(_) => return None,
            Ok(_) => {}
        }
    }
}

impl Plex {
    /// The identifier Plex presents to plex.tv, which plex.tv is asked as.
    async fn presenting(&self) -> Result<String, Failure> {
        self.preferences()
            .await?
            .as_deref()
            .and_then(preferences::client)
            .ok_or_else(|| {
                self.endpoint
                    .refused("Plex's preferences name no identifier for plex.tv")
            })
    }

    /// plex.tv's answer to `method` at `path`, asked as Plex with `token`.
    async fn at_plex_tv(
        &self,
        method: Method,
        path: &str,
        token: &str,
        body: Option<String>,
    ) -> Result<Response, Failure> {
        let mut headers = as_a_server(&self.presenting().await?);
        headers.push((super::TOKEN.to_owned(), token.to_owned()));
        if body.is_some() {
            headers.push(("Content-Type".to_owned(), JSON.to_owned()));
        }
        self.accounts
            .send(&Request {
                method,
                url: self.accounts.url(path),
                headers,
                body,
                pinned: None,
            })
            .await
    }

    /// plex.tv's answer to `method` at `path` as the claiming account, refused where it
    /// is not a success.
    async fn as_owner(
        &self,
        method: Method,
        path: &str,
        body: Option<String>,
    ) -> Result<Response, Failure> {
        let token = self.admin_token().await?;
        let response = self.at_plex_tv(method, path, &token, body).await?;
        if response.is_success() {
            Ok(response)
        } else {
            Err(self.accounts.refusal(&response))
        }
    }

    /// Every account of the claiming account's Plex Home titled `title`.
    async fn home_titled(&self, title: &str) -> Result<Vec<HomeUser>, Failure> {
        let response = self.as_owner(Method::Get, HOME_USERS, None).await?;
        Ok(home_users(&response.body)
            .into_iter()
            .filter(|user| user.title == title)
            .collect())
    }

    /// The token of the Home account `id`, switched to as the claiming account.
    async fn switched_to(&self, id: &str) -> Result<String, Failure> {
        let response = self
            .as_owner(Method::Post, &format!("{HOME_USERS}/{id}/switch"), None)
            .await?;
        switched_token(&response.body).ok_or_else(|| {
            self.accounts
                .refused("plex.tv switched to the account and answered no token")
        })
    }

    /// The account plex.tv answers `token` as, or none where it refuses the token.
    async fn whose(&self, token: &str) -> Result<Option<String>, Failure> {
        let response = self.at_plex_tv(Method::Get, WHO, token, None).await?;
        if response.status == REFUSED_THE_TOKEN {
            return Ok(None);
        }
        if !response.is_success() {
            return Err(self.accounts.refusal(&response));
        }
        let who: Who = serde_json::from_str(&response.body).map_err(|_| {
            self.accounts
                .refused("plex.tv said whose the token is unreadably")
        })?;
        Ok(Some(who.id.to_string()))
    }

    /// Share every library Plex holds with the Home account `id`.
    async fn shared_with(&self, id: &str) -> Result<(), Failure> {
        let served: Served = self
            .read(IDENTITY, "Plex did not say which server it is")
            .await?;
        let sections: Sections = self
            .read(SECTIONS, "the libraries Plex holds could not be read")
            .await?;
        let invited: u64 = id.parse().map_err(|_| {
            self.accounts
                .refused("plex.tv named the account unreadably")
        })?;
        let library_section_ids: Vec<u64> = sections
            .directories
            .iter()
            .filter_map(|section| section.key.parse().ok())
            .collect();
        let machine = served.machine_identifier;
        let body = serde_json::json!({
            "server_id": machine,
            "shared_server": { "library_section_ids": library_section_ids, "invited_id": invited },
        });
        self.as_owner(
            Method::Post,
            &format!("/api/servers/{machine}/shared_servers"),
            Some(body.to_string()),
        )
        .await
        .map(drop)
    }

    /// Remove the Home account `id`.
    async fn removed(&self, id: &str) -> Result<(), Failure> {
        self.as_owner(Method::Delete, &format!("{HOME_USERS}/{id}"), None)
            .await
            .map(drop)
    }

    fn not_an_app(app: &str) -> Failure {
        Failure::Refused {
            service: super::SERVICE.to_owned(),
            detail: format!("{app:?} is not one lowercase word an app's account can be titled for"),
        }
    }
}

#[async_trait]
impl AppKeys for Plex {
    async fn filed_as(&self, app: &str) -> Result<Vec<String>, Failure> {
        let title = titled(app).ok_or_else(|| Self::not_an_app(app))?;
        let mut keys = Vec::new();
        for user in self.home_titled(&title).await? {
            keys.push(self.switched_to(&user.id).await?);
        }
        Ok(keys)
    }

    async fn dated(&self, app: &str) -> Result<Vec<Dated>, Failure> {
        let title = titled(app).ok_or_else(|| Self::not_an_app(app))?;
        Ok(self
            .home_titled(&title)
            .await?
            .iter()
            .map(|_| Dated {
                created: None,
                last_used: None,
            })
            .collect())
    }

    async fn mint(&self, app: &str) -> Result<String, Failure> {
        let title = titled(app).ok_or_else(|| Self::not_an_app(app))?;
        let before = self.home_titled(&title).await?;
        let asked = form_encoded(&[("title", &title)]);
        self.as_owner(Method::Post, &format!("{HOME_USERS}?{asked}"), None)
            .await?;
        let made: Vec<HomeUser> = self
            .home_titled(&title)
            .await?
            .into_iter()
            .filter(|user| !before.contains(user))
            .collect();
        let [made] = made.as_slice() else {
            return Err(self
                .accounts
                .refused("plex.tv did not answer with exactly one new account for the app"));
        };
        let minted = async {
            self.shared_with(&made.id).await?;
            self.switched_to(&made.id).await
        }
        .await;
        if minted.is_err() {
            let _ = self.removed(&made.id).await;
        }
        minted
    }

    async fn answers_to(&self, key: &str) -> Result<(), Failure> {
        let response = self
            .endpoint
            .send(&self.request(Method::Get, SECTIONS, Some(key)))
            .await?;
        if response.status == REFUSED_THE_TOKEN {
            return Err(self.endpoint.unauthorised());
        }
        self.endpoint.expect_success(&response)
    }

    async fn revoke(&self, key: &str) -> Result<(), Failure> {
        self.admin_token().await?;
        let Some(id) = self.whose(key).await? else {
            return Ok(());
        };
        let response = self.as_owner(Method::Get, HOME_USERS, None).await?;
        let home = home_users(&response.body);
        let Some(account) = home.iter().find(|user| user.id == id) else {
            return Err(self
                .accounts
                .refused("the key is no account of the claiming account's Plex Home"));
        };
        if !account.title.starts_with(TITLED) {
            return Err(self
                .accounts
                .refused("the key is a Home account no app's key was made as"));
        }
        self.removed(&account.id).await
    }
}
