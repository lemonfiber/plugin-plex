//! Plex Media Server, asked through its own API with the token it holds for the account
//! that claimed it. Plex writes that token into its `Preferences.xml` once it is claimed,
//! and the adapter reads it from the configuration directory it shares with Plex for
//! every call, holding it no longer than the call.

use std::path::Path;
use std::sync::Arc;

use lemonfiber_adapter_kit::credential::Holding;
use lemonfiber_adapter_kit::endpoint::Endpoint;
use lemonfiber_adapter_kit::shared::{Shared, SharedName};
use lemonfiber_pinned::Https;
use lemonfiber_ports::http::{Fetched, Http, Method, Request, Response, JSON};
use lemonfiber_ports::service::Failure;
use serde::Deserialize;

mod declared;
mod encrypting;
mod furnishing;
mod keys;
mod members;
mod owning;
mod preferences;
mod screening;
mod serving;

pub use declared::presented;

/// What every failure is blamed on.
pub const SERVICE: &str = "plex";

/// What a failure plex.tv answers is blamed on.
pub const PLEX_TV: &str = "plex.tv";

/// Where plex.tv answers.
const PLEX_TV_BASE: &str = "https://plex.tv";

/// The directories, beneath the configuration directory Plex runs with, that hold its
/// `Preferences.xml`.
pub const SUPPORT: [&str; 3] = ["Library", "Application Support", "Plex Media Server"];

const PREFERENCES: &str = "Preferences.xml";

/// The mode Plex keeps its preferences at: its owner's alone.
const OWNER_ONLY: u32 = 0o600;

/// The port Plex serves both plain HTTP and TLS on.
const PORT: u16 = 32_400;

const IDENTITY: &str = "/identity";

const METADATA: &str = "/library/metadata";

const TOKEN: &str = "X-Plex-Token";

const ACCEPT: &str = "Accept";

const ABSENT: u16 = 404;

const LONGEST_ID: usize = 20;

/// A client for one Plex Media Server, asking with the token Plex keeps for the account
/// that claimed it.
pub struct Plex {
    http: Arc<dyn Http>,
    endpoint: Endpoint,
    accounts: Endpoint,
    support: Shared,
    code: Arc<dyn Holding>,
    internal_tls: Option<Https>,
    kept: Option<keys::Kept>,
}

/// What every answer Plex gives in JSON is wrapped in.
#[derive(Deserialize)]
struct Wrapped<T> {
    #[serde(rename = "MediaContainer")]
    container: T,
}

/// What plex.tv is told the asker is: a Plex Media Server, as its image's first run tells
/// it.
const AS_A_SERVER: [(&str, &str); 7] = [
    ("X-Plex-Product", "Plex Media Server"),
    ("X-Plex-Version", "1.1"),
    ("X-Plex-Provides", "server"),
    ("X-Plex-Platform", "Linux"),
    ("X-Plex-Platform-Version", "1.0"),
    ("X-Plex-Device-Name", "PlexMediaServer"),
    ("X-Plex-Device", "Linux"),
];

const CLIENT_HEADER: &str = "X-Plex-Client-Identifier";

/// The headers plex.tv is asked with as the server presenting itself as `client`.
fn as_a_server(client: &str) -> Vec<(String, String)> {
    AS_A_SERVER
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .chain([(CLIENT_HEADER.to_owned(), client.to_owned())])
        .collect()
}

impl Plex {
    /// The Plex reached at `base` over `http`, whose configuration directory is shared
    /// with this adapter at `shared`, claimed with the code `code` holds.
    #[must_use]
    pub fn new(
        http: Arc<dyn Http>,
        base: impl Into<String>,
        shared: &Path,
        code: Arc<dyn Holding>,
    ) -> Self {
        let base = base.into();
        let support = SUPPORT
            .iter()
            .filter_map(|name| SharedName::new(name).ok())
            .fold(Shared::at(shared), |within, name| within.beneath(name));
        Self {
            internal_tls: encrypting::internal_tls(&base),
            endpoint: Endpoint::new(Arc::clone(&http), base, SERVICE),
            accounts: Endpoint::new(Arc::clone(&http), PLEX_TV_BASE, PLEX_TV),
            http,
            support,
            code,
            kept: None,
        }
    }

    /// The same Plex, keeping the accounts it makes as apps' keys in its own
    /// configuration directory `directory`.
    #[must_use]
    pub fn keeping_accounts_in(self, directory: &Path) -> Self {
        Self {
            kept: Some(keys::Kept::in_directory(directory)),
            ..self
        }
    }

    /// The same Plex, its internal TLS port reached at `at` rather than at its own address.
    #[must_use]
    pub fn with_internal_tls(self, at: Https) -> Self {
        Self {
            internal_tls: Some(at),
            ..self
        }
    }

    /// Whether Plex says which server it is, which it says to anybody.
    pub async fn answers(&self) -> bool {
        self.endpoint
            .send(&self.request(Method::Get, IDENTITY, None))
            .await
            .is_ok_and(|response| response.is_success())
    }

    /// What Plex's preferences hold, or nothing where Plex has written none yet.
    async fn preferences(&self) -> Result<Option<Vec<u8>>, Failure> {
        let read = async { self.support.read(&SharedName::new(PREFERENCES)?).await }.await;
        read.map_err(|why| {
            self.endpoint
                .refused(&format!("Plex's preferences could not be read: {why}"))
        })
    }

    /// Write `preferences` as the whole of Plex's preferences, its owner's alone.
    async fn write_preferences(&self, preferences: Vec<u8>) -> Result<(), Failure> {
        let written = async {
            self.support
                .write(&SharedName::new(PREFERENCES)?, preferences, OWNER_ONLY)
                .await
        }
        .await;
        written.map_err(|why| {
            self.endpoint
                .refused(&format!("Plex's preferences could not be written: {why}"))
        })
    }

    /// The token Plex holds for the account that claimed it, or nothing where it holds
    /// none yet.
    async fn token(&self) -> Result<Option<String>, Failure> {
        Ok(self
            .preferences()
            .await?
            .and_then(|bytes| preferences::online_token(&bytes)))
    }

    async fn admin_token(&self) -> Result<String, Failure> {
        self.token()
            .await?
            .ok_or_else(|| self.endpoint.unauthorised())
    }

    fn request(&self, method: Method, path: &str, token: Option<&str>) -> Request {
        let mut headers = vec![(ACCEPT.to_owned(), JSON.to_owned())];
        if let Some(token) = token {
            headers.push((TOKEN.to_owned(), token.to_owned()));
        }
        Request {
            method,
            url: self.endpoint.url(path),
            headers,
            body: None,
            pinned: None,
        }
    }

    async fn as_admin(&self, method: Method, path: &str) -> Result<Response, Failure> {
        let token = self.admin_token().await?;
        self.endpoint
            .send(&self.request(method, path, Some(&token)))
            .await
    }

    async fn fetched_as_admin(&self, path: &str, most: usize) -> Result<Fetched, Failure> {
        let token = self.admin_token().await?;
        self.http
            .fetch(&self.request(Method::Get, path, Some(&token)), most)
            .await
            .map_err(|_| Failure::Unavailable {
                service: SERVICE.to_owned(),
            })
    }

    async fn read<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> Result<T, Failure> {
        let response = self.as_admin(Method::Get, path).await?;
        let wrapped: Wrapped<T> = self.endpoint.decode(&response, what)?;
        Ok(wrapped.container)
    }

    async fn found<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> Result<Option<T>, Failure> {
        let response = self.as_admin(Method::Get, path).await?;
        if response.status == ABSENT {
            return Ok(None);
        }
        let wrapped: Wrapped<T> = self.endpoint.decode(&response, what)?;
        Ok(Some(wrapped.container))
    }

    async fn done(&self, method: Method, path: &str) -> Result<(), Failure> {
        let response = self.as_admin(method, path).await?;
        self.endpoint.expect_success(&response)
    }

    fn unsupported(detail: &str) -> Failure {
        Failure::Unsupported {
            service: SERVICE.to_owned(),
            detail: detail.to_owned(),
        }
    }
}

/// Whether `id` is spelled as the keys Plex files an item under: digits alone.
fn an_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= LONGEST_ID && id.bytes().all(|mark| mark.is_ascii_digit())
}

#[cfg(test)]
mod tests;
