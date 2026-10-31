//! Looking over Plex's libraries again, the series it holds, and reading one item afresh.
//! Plex keeps no list of proxies or origins its API sets, is not asked to start again by
//! this adapter, and holds no keys of its own for an app, so the rest is unsupported.

use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::{AppKeys, Dated, Failure, Fronted, Library, SeriesHeld, Upkeep};

use super::screening::Listed;
use super::{an_id, Plex, ABSENT, METADATA};

const NO_PROXIES: &str = "Plex holds no list of addresses trusted to name the client its API sets";

const NO_ORIGINS: &str = "Plex holds no list of origins a browser may read it from its API sets";

const NO_RESTART: &str = "Plex is not asked to start again by this adapter";

const NO_KEYS: &str =
    "Plex holds no keys of its own for an app: every token it takes is one plex.tv issued";

const NOTHING_THERE: &str = "Plex holds no item under that id";

const FILMS: &str = "1";

const SERIES: &str = "2";

const REFRESH_ALL: &str = "/library/sections/all/refresh";

impl Plex {
    async fn listed(&self, asked: &[(&str, &str)], what: &str) -> Result<Listed, Failure> {
        self.read(&format!("/library/all?{}", form_encoded(asked)), what)
            .await
    }
}

#[async_trait]
impl Library for Plex {
    async fn has_item(&self, kind: Kind, term: &str) -> Result<bool, Failure> {
        let kind = match kind {
            Kind::Movies => FILMS,
            Kind::Tv => SERIES,
        };
        let listed = self
            .listed(
                &[
                    ("type", kind),
                    ("title", term),
                    ("X-Plex-Container-Start", "0"),
                    ("X-Plex-Container-Size", "1"),
                ],
                "whether Plex holds the title could not be read",
            )
            .await?;
        Ok(listed.metadata.iter().any(|one| an_id(&one.rating_key)))
    }

    async fn rescan(&self) -> Result<(), Failure> {
        self.done(Method::Get, REFRESH_ALL).await
    }
}

#[async_trait]
impl Upkeep for Plex {
    async fn series_held(&self, most: u32) -> Result<Vec<SeriesHeld>, Failure> {
        if most == 0 {
            return Ok(Vec::new());
        }
        let listed = self
            .listed(
                &[
                    ("type", SERIES),
                    ("sort", "titleSort"),
                    ("X-Plex-Container-Start", "0"),
                    ("X-Plex-Container-Size", &most.to_string()),
                ],
                "the series Plex holds could not be read",
            )
            .await?;
        Ok(listed
            .metadata
            .into_iter()
            .filter(|one| an_id(&one.rating_key))
            .take(usize::try_from(most).unwrap_or(usize::MAX))
            .map(|one| SeriesHeld {
                seasons: one.child_count.unwrap_or_default(),
                episodes: one.leaf_count.unwrap_or_default(),
                id: one.rating_key,
                title: one.title,
            })
            .collect())
    }

    async fn refresh(&self, id: &str) -> Result<(), Failure> {
        if !an_id(id) {
            return Err(self.endpoint.refused(NOTHING_THERE));
        }
        let response = self
            .as_admin(Method::Put, &format!("{METADATA}/{id}/refresh"))
            .await?;
        if response.status == ABSENT {
            return Err(self.endpoint.refused(NOTHING_THERE));
        }
        self.endpoint.expect_success(&response)
    }
}

#[async_trait]
impl Fronted for Plex {
    async fn known_proxies(&self) -> Result<Vec<String>, Failure> {
        Err(Self::unsupported(NO_PROXIES))
    }

    async fn trust_only(&self, _address: &str) -> Result<(), Failure> {
        Err(Self::unsupported(NO_PROXIES))
    }

    async fn allowed_origins(&self) -> Result<Vec<String>, Failure> {
        Err(Self::unsupported(NO_ORIGINS))
    }

    async fn allow_only(&self, _origin: &str) -> Result<(), Failure> {
        Err(Self::unsupported(NO_ORIGINS))
    }

    async fn restart(&self) -> Result<(), Failure> {
        Err(Self::unsupported(NO_RESTART))
    }
}

#[async_trait]
impl AppKeys for Plex {
    async fn filed_as(&self, _app: &str) -> Result<Vec<String>, Failure> {
        Err(Self::unsupported(NO_KEYS))
    }

    async fn mint(&self, _app: &str) -> Result<String, Failure> {
        Err(Self::unsupported(NO_KEYS))
    }

    async fn dated(&self, _app: &str) -> Result<Vec<Dated>, Failure> {
        Err(Self::unsupported(NO_KEYS))
    }

    async fn answers_to(&self, _key: &str) -> Result<(), Failure> {
        Err(Self::unsupported(NO_KEYS))
    }

    async fn revoke(&self, _key: &str) -> Result<(), Failure> {
        Err(Self::unsupported(NO_KEYS))
    }
}
