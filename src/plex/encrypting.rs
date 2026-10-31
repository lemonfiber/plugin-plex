//! The internal TLS port: Plex serves TLS on its own port with the PKCS#12 its
//! `customCertificatePath` preference names, read when it starts, so the certificate the
//! core makes is served once Plex restarts with that preference set.

use std::time::Duration;

use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_pinned::Https;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Applied, Encrypting, Failure};
use serde::Deserialize;

use super::{Plex, PORT};

const PREFS: &str = "/:/prefs";

const CERTIFICATE_PATH: &str = "customCertificatePath";

const CERTIFICATE_KEY: &str = "customCertificateKey";

const WHAT: &str = "Plex's preferences could not be read";

const HANDSHAKE_WITHIN: Duration = Duration::from_secs(5);

const PIN_DIGITS: usize = 64;

#[derive(Deserialize)]
struct Settings {
    #[serde(rename = "Setting", default)]
    settings: Vec<Setting>,
}

#[derive(Deserialize)]
struct Setting {
    id: String,
    #[serde(default)]
    value: serde_json::Value,
}

impl Settings {
    fn value(&self, id: &str) -> Option<&str> {
        self.settings
            .iter()
            .find(|setting| setting.id == id)
            .and_then(|setting| setting.value.as_str())
    }

    fn serving(&self, path: &str) -> bool {
        self.value(CERTIFICATE_PATH) == Some(path) && self.value(CERTIFICATE_KEY) == Some("")
    }
}

/// Where Plex reached at `base` serves TLS.
pub(super) fn internal_tls(base: &str) -> Option<Https> {
    reqwest::Url::parse(base)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .and_then(|host| Https::parse(&format!("https://{host}:{PORT}")))
}

fn pinned(pin: &str) -> bool {
    pin.len() == PIN_DIGITS
        && pin
            .bytes()
            .all(|digit| digit.is_ascii_digit() || (b'a'..=b'f').contains(&digit))
}

impl Plex {
    async fn settings(&self) -> Result<Settings, Failure> {
        self.read(PREFS, WHAT).await
    }

    async fn presents(&self, pin: &str) -> bool {
        match &self.internal_tls {
            Some(at) => lemonfiber_pinned::presents(at, pin, HANDSHAKE_WITHIN).await,
            None => false,
        }
    }
}

#[async_trait]
impl Encrypting for Plex {
    async fn serve_tls(&self, path: &str, pin: &str) -> Result<Applied, Failure> {
        if path.trim().is_empty() {
            return Err(self
                .endpoint
                .refused("no certificate path was given to serve the internal TLS port with"));
        }
        if !pinned(pin) {
            return Err(self.endpoint.refused(&format!(
                "{pin:?} is not a certificate's SHA-256 in lower-case hexadecimal"
            )));
        }
        if self.settings().await?.serving(path) {
            return Ok(if self.presents(pin).await {
                Applied::Now
            } else {
                Applied::OnRestart
            });
        }
        let asked = form_encoded(&[(CERTIFICATE_PATH, path), (CERTIFICATE_KEY, "")]);
        self.done(Method::Put, &format!("{PREFS}?{asked}")).await?;
        if !self.settings().await?.serving(path) {
            return Err(self.endpoint.refused(&format!(
                "Plex was asked to serve the internal TLS port with {path} and reads its preferences back otherwise"
            )));
        }
        Ok(Applied::OnRestart)
    }
}
