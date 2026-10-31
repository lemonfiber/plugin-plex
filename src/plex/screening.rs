//! What Plex holds, read as the account that claimed it reads it: each film and series
//! as an item, a series with its seasons and their episodes, and what is playing now.
//! A member of the household is a plex.tv account `identity.source` names, and what is
//! asked for one is answered by that account's own token on this server, which only
//! plex.tv hands out; until the adapter asks plex.tv for it, everything asked for one
//! member is unsupported.

use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_adapter_kit::picture;
use lemonfiber_ports::service::{
    EpisodeDetail, Failure, Holds, HowFar, Image, Item, ItemDetail, ItemProgress, Medium, Picture,
    Playback, Screening, SeasonDetail, PICTURE_MOST,
};
use serde::Deserialize;

use super::{an_id, Plex, ABSENT, METADATA};

const NO_MEMBERS: &str =
    "a member's own token on Plex is handed out by plex.tv, which this adapter does not ask yet";

const NO_DEVICES: &str = "Plex opens and ends no device's session for lemonfiber";

/// Films and series, by the type numbers Plex files them under.
const FILMS_AND_SERIES: &str = "1,2";

const BY_TITLE: &str = "titleSort";

const SESSIONS: &str = "/status/sessions";

const DATE_LENGTH: usize = 10;

const MILLISECONDS_A_MINUTE: u64 = 60_000;

#[derive(Deserialize, Default)]
pub(super) struct Listed {
    #[serde(rename = "Metadata", default)]
    pub(super) metadata: Vec<Metadata>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct Metadata {
    pub(super) rating_key: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    pub(super) title: String,
    #[serde(default)]
    year: Option<u16>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    duration: Option<u64>,
    #[serde(rename = "Genre", default)]
    genres: Vec<Tag>,
    #[serde(default)]
    content_rating: Option<String>,
    #[serde(default)]
    originally_available_at: Option<String>,
    #[serde(default)]
    thumb: Option<String>,
    #[serde(default)]
    art: Option<String>,
    #[serde(default)]
    index: Option<u32>,
    #[serde(default)]
    parent_index: Option<u32>,
    #[serde(default)]
    grandparent_title: Option<String>,
    #[serde(default)]
    pub(super) child_count: Option<u32>,
    #[serde(default)]
    pub(super) leaf_count: Option<u32>,
    #[serde(rename = "User", default)]
    user: Option<Account>,
    #[serde(rename = "Player", default)]
    player: Option<Player>,
}

#[derive(Deserialize, Default)]
struct Tag {
    tag: String,
}

#[derive(Deserialize, Default)]
struct Account {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize, Default)]
struct Player {
    #[serde(default)]
    title: String,
    #[serde(default)]
    state: String,
}

/// Whether `said` is a calendar date written as `YYYY-MM-DD`.
fn a_date(said: &str) -> bool {
    said.len() == DATE_LENGTH
        && said.bytes().enumerate().all(|(at, mark)| match at {
            4 | 7 => mark == b'-',
            _ => mark.is_ascii_digit(),
        })
}

impl Metadata {
    fn medium(&self) -> Medium {
        match self.kind.as_str() {
            "movie" => Medium::Film,
            "show" => Medium::Series,
            "episode" => Medium::Episode,
            _ => Medium::Other,
        }
    }

    fn plays(&self) -> bool {
        matches!(self.kind.as_str(), "movie" | "episode" | "track" | "clip")
    }

    pub(super) fn item(&self) -> Item {
        Item {
            id: self.rating_key.clone(),
            title: self.title.clone(),
            year: self.year,
            medium: self.medium(),
            holds: Holds {
                poster: self.thumb.is_some(),
                backdrop: self.art.is_some(),
                plays: self.plays(),
            },
        }
    }

    fn overview(&self) -> Option<String> {
        self.summary.clone().filter(|said| !said.trim().is_empty())
    }

    fn minutes(&self) -> Option<u32> {
        self.duration
            .map(|milliseconds| milliseconds.div_ceil(MILLISECONDS_A_MINUTE))
            .and_then(|minutes| u32::try_from(minutes).ok())
    }

    /// The certificate, without the country Plex names it for where it names one.
    fn certificate(&self) -> Option<String> {
        self.content_rating
            .as_deref()
            .map(|rating| {
                rating
                    .rsplit('/')
                    .next()
                    .unwrap_or(rating)
                    .trim()
                    .to_owned()
            })
            .filter(|rating| !rating.is_empty())
    }

    fn released(&self) -> Option<String> {
        self.originally_available_at
            .clone()
            .filter(|date| a_date(date))
    }

    fn detail(self, seasons: Vec<SeasonDetail>) -> ItemDetail {
        ItemDetail {
            item: self.item(),
            overview: self.overview(),
            minutes: self.minutes(),
            certificate: self.certificate(),
            released: self.released(),
            genres: self.genres.into_iter().map(|genre| genre.tag).collect(),
            seasons,
        }
    }

    fn episode(self) -> EpisodeDetail {
        EpisodeDetail {
            item: self.item(),
            number: self.index,
            overview: self.overview(),
            minutes: self.minutes(),
        }
    }

    fn playback(self) -> Option<Playback> {
        let user = self.user.as_ref()?;
        let player = self.player.as_ref()?;
        let episode = self.kind == "episode";
        Some(Playback {
            member_id: user.id.clone(),
            member: user.title.clone(),
            medium: self.medium(),
            paused: player.state == "paused",
            device: player.title.clone(),
            series: self.grandparent_title.clone().filter(|_| episode),
            season: self.parent_index.filter(|_| episode),
            episode: self.index.filter(|_| episode),
            title: self.title,
        })
    }
}

impl Plex {
    async fn children(&self, id: &str) -> Result<Vec<Metadata>, Failure> {
        let listed: Listed = self
            .read(
                &format!("{METADATA}/{id}/children"),
                "what Plex files under an item could not be read",
            )
            .await?;
        Ok(listed
            .metadata
            .into_iter()
            .filter(|one| an_id(&one.rating_key))
            .collect())
    }

    async fn seasons(&self, id: &str) -> Result<Vec<SeasonDetail>, Failure> {
        let mut seasons = Vec::new();
        for season in self.children(id).await? {
            let episodes = self
                .children(&season.rating_key)
                .await?
                .into_iter()
                .map(Metadata::episode)
                .collect();
            seasons.push(SeasonDetail {
                id: season.rating_key,
                name: season.title,
                number: season.index,
                episodes,
            });
        }
        Ok(seasons)
    }

    async fn titled(&self, id: &str) -> Result<Option<ItemDetail>, Failure> {
        if !an_id(id) {
            return Ok(None);
        }
        let Some(listed) = self
            .found::<Listed>(
                &format!("{METADATA}/{id}"),
                "the item Plex holds could not be read",
            )
            .await?
        else {
            return Ok(None);
        };
        let Some(held) = listed.metadata.into_iter().find(|one| one.rating_key == id) else {
            return Ok(None);
        };
        let seasons = if held.medium() == Medium::Series {
            self.seasons(id).await?
        } else {
            Vec::new()
        };
        Ok(Some(held.detail(seasons)))
    }

    async fn pictured(&self, id: &str, which: Picture) -> Result<Option<Image>, Failure> {
        if !an_id(id) {
            return Ok(None);
        }
        let shown = match which {
            Picture::Poster => "thumb",
            Picture::Backdrop => "art",
        };
        let fetched = self
            .fetched_as_admin(&format!("{METADATA}/{id}/{shown}"), PICTURE_MOST)
            .await?;
        if fetched.status == ABSENT {
            return Ok(None);
        }
        if !fetched.is_success() {
            return Err(self.endpoint.refused("Plex would not answer the picture"));
        }
        let served = fetched.header("content-type").map(str::to_owned);
        Ok(fetched
            .bytes
            .and_then(|bytes| picture::image(served.as_deref(), bytes)))
    }

    async fn held(&self, most: u32) -> Result<Vec<Item>, Failure> {
        if most == 0 {
            return Ok(Vec::new());
        }
        let asked = form_encoded(&[
            ("type", FILMS_AND_SERIES),
            ("sort", BY_TITLE),
            ("X-Plex-Container-Start", "0"),
            ("X-Plex-Container-Size", &most.to_string()),
        ]);
        let listed: Listed = self
            .read(
                &format!("/library/all?{asked}"),
                "what Plex holds could not be read",
            )
            .await?;
        Ok(listed
            .metadata
            .iter()
            .filter(|one| an_id(&one.rating_key))
            .take(usize::try_from(most).unwrap_or(usize::MAX))
            .map(Metadata::item)
            .collect())
    }

    async fn sessions(&self, member: Option<&str>) -> Result<Vec<Playback>, Failure> {
        let listed: Listed = self
            .read(SESSIONS, "what Plex is playing could not be read")
            .await?;
        Ok(listed
            .metadata
            .into_iter()
            .filter_map(Metadata::playback)
            .filter(|playing| member.is_none_or(|member| playing.member_id == member))
            .collect())
    }
}

#[async_trait]
impl Screening for Plex {
    async fn signed_in(&self, _member: &str, _device: &str) -> Result<Option<String>, Failure> {
        Err(Self::unsupported(NO_MEMBERS))
    }

    async fn title(&self, member: Option<&str>, id: &str) -> Result<Option<ItemDetail>, Failure> {
        if member.is_some() {
            return Err(Self::unsupported(NO_MEMBERS));
        }
        self.titled(id).await
    }

    async fn part_way(&self, _member: &str, _most: u32) -> Result<Vec<ItemProgress>, Failure> {
        Err(Self::unsupported(NO_MEMBERS))
    }

    async fn progressed(&self, _member: &str, _id: &str, _how_far: &HowFar) -> Result<(), Failure> {
        Err(Self::unsupported(NO_MEMBERS))
    }

    async fn sign_out(&self, _device: &str) -> Result<(), Failure> {
        Err(Self::unsupported(NO_DEVICES))
    }

    async fn picture(
        &self,
        member: Option<&str>,
        id: &str,
        which: Picture,
    ) -> Result<Option<Image>, Failure> {
        if member.is_some() {
            return Err(Self::unsupported(NO_MEMBERS));
        }
        self.pictured(id, which).await
    }

    async fn holdings(&self, member: Option<&str>, most: u32) -> Result<Vec<Item>, Failure> {
        if member.is_some() {
            return Err(Self::unsupported(NO_MEMBERS));
        }
        self.held(most).await
    }

    async fn playing(&self, member: Option<&str>) -> Result<Vec<Playback>, Failure> {
        self.sessions(member).await
    }
}
