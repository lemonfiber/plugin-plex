//! Plex's live TV, asked for one member with their own server token (ARCH-R421, ARCH-R436):
//! the channels of each DVR's lineup, each channel's guide read one day at a time from its
//! DVR's guide provider, a channel tuned on its DVR, and a live session terminated.

use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::{Method, Response};
use lemonfiber_ports::instant::{Instant, DAY, HOUR};
use lemonfiber_ports::service::{
    Airing, Channel, Failure, Listing, Programme, Tuned, Tuning, GUIDE_HOURS,
};
use serde::Deserialize;

use super::household::Watcher;
use super::{Plex, Wrapped, ABSENT};

mod opened;

pub(super) use opened::Opened;

const DVRS: &str = "/livetv/dvrs";

const PROVIDER: &str = "tv.plex.providers.epg.";

const LINEUP: &str = "lineup://";

const TERMINATE: &str = "/status/sessions/terminate";

const SESSIONS: &str = "/livetv/sessions";

const TRANSCODER: &str = "/video/:/transcode/universal/start.m3u8";

const CONSUMER: &str = "lemonfiber";

const STOPPED: &str = "The live stream was closed";

const REFUSED_THE_ASK: std::ops::Range<u16> = 400..500;

const UNREAD_DVRS: &str = "Plex did not say which DVRs it holds";

const NO_DVR: &str = "Plex holds no DVR with a guide, so it has no live TV";

const UNREAD_CHANNELS: &str = "Plex did not say which channels a DVR's lineup holds";

const UNREAD_GUIDE: &str = "Plex did not answer a day of a channel's guide";

const UNREAD_TUNED: &str = "Plex tuned the channel and named no live session";

const STREAM_SHAPE: usize = 36;

const UNKEPT: &str = "this adapter keeps no record of the live sessions it tunes";

const NOT_TUNED_HERE: &str = "a live stream is closed by the id tune answered";

#[derive(Deserialize, Default)]
struct Dvrs {
    #[serde(rename = "Dvr", alias = "DVR", default)]
    dvrs: Vec<Dvr>,
}

#[derive(Deserialize)]
struct Dvr {
    key: serde_json::Value,
    #[serde(default)]
    lineup: String,
}

#[derive(Deserialize, Default)]
struct Lineup {
    #[serde(rename = "Channel", default)]
    channels: Vec<LineupChannel>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LineupChannel {
    grid_key: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    call_sign: Option<String>,
    #[serde(default)]
    vcn: Option<String>,
}

#[derive(Deserialize, Default)]
struct Grid {
    #[serde(rename = "Metadata", default)]
    metadata: Vec<Aired>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Aired {
    #[serde(default)]
    title: String,
    #[serde(default)]
    grandparent_title: Option<String>,
    #[serde(default)]
    content_rating: Option<String>,
    #[serde(rename = "Label", default)]
    labels: Vec<Tag>,
    #[serde(rename = "Media", default)]
    media: Vec<AiredMedia>,
}

#[derive(Deserialize)]
struct Tag {
    #[serde(default)]
    tag: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiredMedia {
    #[serde(default)]
    begins_at: Option<u64>,
    #[serde(default)]
    ends_at: Option<u64>,
}

#[derive(Deserialize, Default)]
struct TunedSession {
    #[serde(rename = "Metadata", default)]
    metadata: Vec<TunedEntry>,
}

#[derive(Deserialize)]
struct TunedEntry {
    #[serde(rename = "Media", default)]
    media: Vec<TunedMedia>,
}

#[derive(Deserialize)]
struct TunedMedia {
    #[serde(default)]
    uuid: Option<String>,
}

/// One channel of one DVR's lineup.
struct Lined {
    dvr: String,
    provider: String,
    channel: LineupChannel,
}

impl Lined {
    fn id(&self) -> String {
        format!("{}-{}", self.dvr, self.channel.grid_key)
    }
}

/// One programme of a channel's guide, as Plex holds it.
struct Held {
    title: String,
    certificate: Option<String>,
    labels: Vec<String>,
    starts: u64,
    ends: u64,
}

impl Held {
    fn airing(self) -> Airing {
        Airing {
            title: self.title,
            certificate: self.certificate,
            starts: Instant::from_seconds(self.starts),
            ends: Instant::from_seconds(self.ends),
        }
    }
}

fn now() -> Instant {
    Instant::of(std::time::SystemTime::now())
}

fn a_stream(id: &str) -> bool {
    id.len() == STREAM_SHAPE
        && id
            .bytes()
            .all(|mark| matches!(mark, b'0'..=b'9' | b'a'..=b'f' | b'-'))
}

fn a_number(number: &str) -> bool {
    !number.is_empty()
        && number
            .bytes()
            .all(|mark| mark.is_ascii_digit() || mark == b'.')
}

fn dvr_key(key: &serde_json::Value) -> Option<String> {
    match key {
        serde_json::Value::String(key) => Some(key.clone()),
        serde_json::Value::Number(key) => Some(key.to_string()),
        _ => None,
    }
    .filter(|key| super::an_id(key))
}

/// The guide provider a DVR's lineup is read from, by the kind its lineup names.
fn provider(dvr: &str, lineup: &str) -> Option<String> {
    let kind = lineup
        .strip_prefix(LINEUP)?
        .strip_prefix(PROVIDER)?
        .split(['/', '#', '?'])
        .next()
        .filter(|kind| !kind.is_empty() && kind.bytes().all(|mark| mark.is_ascii_lowercase()))?;
    Some(format!("/{PROVIDER}{kind}:{dvr}"))
}

/// Each day, by its date in UTC, that the hours from `from` to `until` fall on.
fn days(from: Instant, until: Instant) -> Vec<String> {
    (from.seconds() / DAY..=until.seconds() / DAY)
        .filter_map(|day| Instant::from_seconds(day * DAY).day())
        .map(|day| day.to_string())
        .collect()
}

impl Plex {
    async fn as_member(
        &self,
        token: &str,
        method: Method,
        path: &str,
    ) -> Result<Response, Failure> {
        self.endpoint
            .send(&self.request(method, path, Some(token)))
            .await
    }

    async fn decoded<T: serde::de::DeserializeOwned + Default>(
        &self,
        token: &str,
        path: &str,
        what: &str,
    ) -> Result<T, Failure> {
        let response = self.as_member(token, Method::Get, path).await?;
        let wrapped: Wrapped<T> = self.endpoint.decode(&response, what)?;
        Ok(wrapped.container)
    }

    /// Each DVR Plex holds, by its key, with the guide provider its lineup is read from;
    /// unsupported where it holds none, as Plex without live TV does.
    async fn dvrs(&self) -> Result<Vec<(String, String)>, Failure> {
        let held: Dvrs = self.found(DVRS, UNREAD_DVRS).await?.unwrap_or_default();
        let dvrs: Vec<(String, String)> = held
            .dvrs
            .iter()
            .filter_map(|dvr| {
                let key = dvr_key(&dvr.key)?;
                let provider = provider(&key, &dvr.lineup)?;
                Some((key, provider))
            })
            .collect();
        if dvrs.is_empty() {
            return Err(Self::unsupported(NO_DVR));
        }
        Ok(dvrs)
    }

    /// Every channel of each of `dvrs`' lineups `watcher` is shown.
    async fn lineup(
        &self,
        dvrs: Vec<(String, String)>,
        watcher: &Watcher,
    ) -> Result<Vec<Lined>, Failure> {
        let mut lined = Vec::new();
        for (key, provider) in dvrs {
            let lineup: Lineup = self
                .decoded(
                    &watcher.token,
                    &format!("{provider}/lineups/dvr/channels"),
                    UNREAD_CHANNELS,
                )
                .await?;
            lined.extend(lineup.channels.into_iter().map(|channel| Lined {
                dvr: key.clone(),
                provider: provider.clone(),
                channel,
            }));
        }
        Ok(lined)
    }

    /// What `channel`'s guide holds from `from` to `until`, read one day at a time with
    /// `token`, each programme overlapping those hours, in the order they air; a day that
    /// cannot be read refuses it.
    async fn scheduled(
        &self,
        token: &str,
        channel: &Lined,
        from: Instant,
        until: Instant,
    ) -> Result<Vec<Held>, Failure> {
        let mut held = Vec::new();
        for day in days(from, until) {
            let asked = form_encoded(&[
                ("channelGridKey", channel.channel.grid_key.as_str()),
                ("date", day.as_str()),
            ]);
            let grid: Grid = self
                .decoded(
                    token,
                    &format!("{}/grid?{asked}", channel.provider),
                    UNREAD_GUIDE,
                )
                .await?;
            for aired in grid.metadata {
                for media in &aired.media {
                    let (Some(starts), Some(ends)) = (media.begins_at, media.ends_at) else {
                        continue;
                    };
                    if ends <= from.seconds() || starts >= until.seconds() || ends <= starts {
                        continue;
                    }
                    held.push(Held {
                        title: aired
                            .grandparent_title
                            .clone()
                            .filter(|title| !title.is_empty())
                            .unwrap_or_else(|| aired.title.clone()),
                        certificate: aired
                            .content_rating
                            .clone()
                            .filter(|rated| !rated.is_empty()),
                        labels: aired.labels.iter().map(|label| label.tag.clone()).collect(),
                        starts,
                        ends,
                    });
                }
            }
        }
        held.sort_by_key(|programme| programme.starts);
        held.dedup_by_key(|programme| (programme.starts, programme.ends));
        Ok(held)
    }

    /// Where the live sessions this adapter tuned are kept, refused where nothing keeps
    /// them.
    fn opened(&self) -> Result<&Opened, Failure> {
        self.opened
            .as_ref()
            .ok_or_else(|| self.endpoint.refused(UNKEPT))
    }

    /// The live session `stream` terminated; one Plex no longer holds is closed already.
    async fn terminated(&self, stream: &str) -> Result<(), Failure> {
        let path = format!(
            "{TERMINATE}?{}",
            form_encoded(&[("sessionId", stream), ("reason", STOPPED)])
        );
        let response = self.as_admin(Method::Post, &path).await?;
        if response.status == ABSENT {
            return Ok(());
        }
        self.endpoint.expect_success(&response)
    }

    /// Each channel of `dvrs`' lineups `watcher` may watch at `at`, with the programme on
    /// it then: one whose programme on now their share or restriction profile keeps from
    /// them is left out, and for a member held to anything, so is one with nothing known
    /// to be airing.
    async fn watched(
        &self,
        dvrs: Vec<(String, String)>,
        watcher: &Watcher,
        at: Instant,
    ) -> Result<Vec<(Lined, Option<Held>)>, Failure> {
        let mut open = Vec::new();
        for lined in self.lineup(dvrs, watcher).await? {
            let shown = self
                .scheduled(&watcher.token, &lined, at, at.later(1))
                .await?
                .into_iter()
                .next();
            let kept = match &shown {
                Some(programme) => {
                    watcher.admits(programme.certificate.as_deref(), &programme.labels)
                }
                None => !watcher.restricted(),
            };
            if kept {
                open.push((lined, shown));
            }
        }
        Ok(open)
    }
}

#[async_trait]
impl Tuning for Plex {
    async fn channels(&self, member: &str) -> Result<Vec<Channel>, Failure> {
        let dvrs = self.dvrs().await?;
        let Some(watcher) = self.watcher(member).await? else {
            return Ok(Vec::new());
        };
        let mut answered = Vec::new();
        for (lined, on_now) in self.watched(dvrs, &watcher, now()).await? {
            answered.push(Channel {
                id: lined.id(),
                name: if lined.channel.title.is_empty() {
                    lined.channel.call_sign.clone().unwrap_or_default()
                } else {
                    lined.channel.title.clone()
                },
                number: lined
                    .channel
                    .vcn
                    .clone()
                    .filter(|number| !number.is_empty()),
                now: on_now.map(|programme| Programme {
                    title: programme.title,
                    certificate: programme.certificate,
                    ends: Some(Instant::from_seconds(programme.ends)),
                }),
            });
        }
        Ok(answered)
    }

    async fn guide(&self, member: &str, hours: u32) -> Result<Vec<Listing>, Failure> {
        if !GUIDE_HOURS.contains(&hours) {
            return Err(self.endpoint.refused(&format!(
                "a guide is asked across {} to {} hours",
                GUIDE_HOURS.start(),
                GUIDE_HOURS.end()
            )));
        }
        let dvrs = self.dvrs().await?;
        let Some(watcher) = self.watcher(member).await? else {
            return Ok(Vec::new());
        };
        let from = now();
        let until = from.later(u64::from(hours) * HOUR);
        let owner = self.admin_token().await?;
        let mut listings = Vec::new();
        for (lined, _) in self.watched(dvrs, &watcher, from).await? {
            let whole = self.scheduled(&owner, &lined, from, until).await?;
            let kept = self.scheduled(&watcher.token, &lined, from, until).await?;
            listings.push(Listing {
                channel: lined.id(),
                until: whole
                    .iter()
                    .map(|programme| programme.ends)
                    .max()
                    .map(Instant::from_seconds),
                programmes: kept
                    .into_iter()
                    .filter(|programme| {
                        watcher.admits(programme.certificate.as_deref(), &programme.labels)
                    })
                    .map(Held::airing)
                    .collect(),
            });
        }
        Ok(listings)
    }

    async fn tune(&self, member: &str, channel: &str) -> Result<Option<Tuned>, Failure> {
        let opened = self.opened()?;
        let dvrs = self.dvrs().await?;
        let Some(watcher) = self.watcher(member).await? else {
            return Ok(None);
        };
        let Some((lined, _)) = self
            .watched(dvrs, &watcher, now())
            .await?
            .into_iter()
            .find(|(lined, _)| lined.id() == channel)
        else {
            return Ok(None);
        };
        let Some(number) = lined
            .channel
            .vcn
            .as_deref()
            .filter(|number| a_number(number))
        else {
            return Ok(None);
        };
        let path = format!("{DVRS}/{}/channels/{number}/tune", lined.dvr);
        let mut request = self.request(Method::Post, &path, Some(&watcher.token));
        request
            .headers
            .push((super::CLIENT_HEADER.to_owned(), CONSUMER.to_owned()));
        let response = self.endpoint.send(&request).await?;
        if REFUSED_THE_ASK.contains(&response.status) {
            return Ok(None);
        }
        let tuned: Wrapped<TunedSession> = self.endpoint.decode(&response, UNREAD_TUNED)?;
        let stream = tuned
            .container
            .metadata
            .into_iter()
            .flat_map(|entry| entry.media)
            .find_map(|media| media.uuid.filter(|uuid| a_stream(uuid)))
            .ok_or_else(|| self.endpoint.refused(UNREAD_TUNED))?;
        if let Err(why) = opened.tuned(&stream) {
            let _ = self.terminated(&stream).await;
            return Err(self.endpoint.refused(&why));
        }
        let direct = format!("{SESSIONS}/{stream}/{CONSUMER}/index.m3u8");
        let transcoded = format!(
            "{TRANSCODER}?{}",
            form_encoded(&[
                ("session", stream.as_str()),
                ("path", direct.as_str()),
                ("protocol", "hls"),
                ("directStream", "1"),
            ])
        );
        Ok(Some(Tuned {
            closes_by: stream,
            direct,
            transcoded,
        }))
    }

    async fn untune(&self, closes_by: &str) -> Result<(), Failure> {
        let opened = self.opened()?;
        let tuned_here = a_stream(closes_by)
            && opened
                .holds(closes_by)
                .map_err(|why| self.endpoint.refused(&why))?;
        if !tuned_here {
            return Err(self.endpoint.refused(NOT_TUNED_HERE));
        }
        self.terminated(closes_by).await?;
        opened
            .closed(closes_by)
            .map_err(|why| self.endpoint.refused(&why))
    }
}
