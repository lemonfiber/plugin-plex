//! A library for each media type the stack files that Plex takes, added where its folder
//! exists and no library reads it, and none Plex already holds renamed, re-pointed or
//! removed.

use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, FiledAt, Furnished, Furnishing};
use serde::Deserialize;

use super::Plex;

const SECTIONS: &str = "/library/sections";

const LANGUAGE: &str = "en-US";

const REFUSED_THE_FOLDER: std::ops::Range<u16> = 400..500;

/// How Plex makes a library of one media type: its title, section type, agent and scanner.
struct Kind {
    media_type: &'static str,
    title: &'static str,
    section: &'static str,
    agent: &'static str,
    scanner: &'static str,
}

const TAKEN: [Kind; 3] = [
    Kind {
        media_type: "movies",
        title: "Movies",
        section: "movie",
        agent: "tv.plex.agents.movie",
        scanner: "Plex Movie",
    },
    Kind {
        media_type: "tv",
        title: "TV",
        section: "show",
        agent: "tv.plex.agents.series",
        scanner: "Plex TV Series",
    },
    Kind {
        media_type: "music",
        title: "Music",
        section: "artist",
        agent: "tv.plex.agents.music",
        scanner: "Plex Music",
    },
];

#[derive(Deserialize, Default)]
struct Sections {
    #[serde(rename = "Directory", default)]
    directories: Vec<Directory>,
}

#[derive(Deserialize)]
struct Directory {
    #[serde(rename = "Location", default)]
    locations: Vec<Location>,
}

#[derive(Deserialize)]
struct Location {
    path: String,
}

impl Sections {
    fn reads(&self, folder: &str) -> bool {
        self.directories
            .iter()
            .flat_map(|directory| &directory.locations)
            .any(|location| location.path.trim_end_matches('/') == folder.trim_end_matches('/'))
    }
}

impl Plex {
    /// Add a library of `kind` reading `folder`: whether Plex now reads it, Plex refusing
    /// a folder it cannot read answered as not.
    async fn added(&self, kind: &Kind, folder: &str) -> Result<bool, Failure> {
        let asked = form_encoded(&[
            ("name", kind.title),
            ("type", kind.section),
            ("agent", kind.agent),
            ("scanner", kind.scanner),
            ("language", LANGUAGE),
            ("location", folder),
        ]);
        let response = self
            .as_admin(Method::Post, &format!("{SECTIONS}?{asked}"))
            .await?;
        if REFUSED_THE_FOLDER.contains(&response.status) {
            return Ok(false);
        }
        self.endpoint.expect_success(&response).map(|()| true)
    }
}

#[async_trait]
impl Furnishing for Plex {
    async fn furnish(&self, filed: &[FiledAt]) -> Result<Vec<Furnished>, Failure> {
        let mut sections: Sections = self
            .read(SECTIONS, "the libraries Plex holds could not be read")
            .await?;
        let mut furnished = Vec::with_capacity(filed.len());
        for one in filed {
            let held = match TAKEN.iter().find(|kind| kind.media_type == one.media_type) {
                None => false,
                Some(_) if sections.reads(&one.folder) => true,
                Some(kind) => {
                    let added = self.added(kind, &one.folder).await?;
                    if added {
                        sections.directories.push(Directory {
                            locations: vec![Location {
                                path: one.folder.clone(),
                            }],
                        });
                    }
                    added
                }
            };
            furnished.push(Furnished {
                media_type: one.media_type.clone(),
                held,
            });
        }
        Ok(furnished)
    }
}
