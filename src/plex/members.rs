//! The guard's question about one item, answered by asking Plex with the member's own
//! token, so Plex decides what that member may have.

use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Answered, Failure};
use serde::Deserialize;

use super::declared::{ITEM_SPACE, PART};
use super::{an_id, Plex, Wrapped, METADATA};

const FILMS_AND_EPISODES: &str = "1,4";

const REFUSED_THE_TOKEN: u16 = 401;

const REFUSED_THE_ASK: std::ops::Range<u16> = 400..500;

const UNREAD: &str = "Plex would not say whether the member may have the item";

#[derive(Deserialize, Default)]
struct Shown {
    #[serde(rename = "Metadata", default)]
    metadata: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    #[serde(default)]
    rating_key: Option<String>,
    #[serde(rename = "Media", default)]
    media: Vec<Media>,
}

#[derive(Deserialize)]
struct Media {
    #[serde(rename = "Part", default)]
    part: Vec<Part>,
}

#[derive(Deserialize)]
struct Part {
    id: Option<u64>,
}

/// Whether an entry Plex shows the member is the item asked about, by its id and the
/// number that id spells.
type Granted = fn(&Entry, &str, u64) -> bool;

impl Entry {
    fn holds_part(&self, part: u64) -> bool {
        self.media
            .iter()
            .flat_map(|media| &media.part)
            .any(|held| held.id == Some(part))
    }
}

impl Plex {
    /// Whether the member whose token is `token` may have the bytes of `item`, an id in
    /// `space`, as Plex answers that member: only where Plex shows that member the item
    /// itself, or an item holding that very part.
    ///
    /// # Errors
    ///
    /// Where Plex does not answer, or answers in a way no question to it expects.
    pub async fn may(&self, space: &str, item: &str, token: &str) -> Result<Answered, Failure> {
        let Some(number) = an_id(item).then(|| item.parse::<u64>().ok()).flatten() else {
            return Ok(Answered::MayNot);
        };
        let (path, granted): (String, Granted) = match space {
            PART => (
                format!(
                    "/library/all?{}",
                    form_encoded(&[
                        ("type", FILMS_AND_EPISODES),
                        ("part", item),
                        ("X-Plex-Container-Start", "0"),
                        ("X-Plex-Container-Size", "1"),
                    ])
                ),
                |entry, _, part| entry.holds_part(part),
            ),
            ITEM_SPACE => (format!("{METADATA}/{item}"), |entry, item, _| {
                entry.rating_key.as_deref() == Some(item)
            }),
            _ => return Ok(Answered::MayNot),
        };
        let response = self
            .endpoint
            .send(&self.request(Method::Get, &path, Some(token)))
            .await?;
        if response.status == REFUSED_THE_TOKEN {
            return Ok(Answered::NoSession);
        }
        if REFUSED_THE_ASK.contains(&response.status) {
            return Ok(Answered::MayNot);
        }
        if !response.is_success() {
            return Err(self.endpoint.refused(UNREAD));
        }
        let shown: Wrapped<Shown> = self.endpoint.decode(&response, UNREAD)?;
        let held = shown
            .container
            .metadata
            .iter()
            .any(|entry| granted(entry, item, number));
        Ok(if held {
            Answered::May
        } else {
            Answered::MayNot
        })
    }
}
