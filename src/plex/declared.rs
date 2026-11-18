//! What a guard in front of Plex enforces: the paths Plex serves an item's bytes under,
//! each naming the kind of id the adapter is asked about it in, and how a member's token
//! reaches Plex.

use async_trait::async_trait;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    Addressee, Answers, Bound, BytePath, Declared, Declaring, Failure, InQuery, LivePath,
    Locations, Ports, Presentation, Question, Token, BASE, ITEM, MAY,
};

use super::{Plex, LONGEST_ID, METADATA, PORT, TOKEN};

/// The space a part's id is in: the file one version of an item is kept in.
pub(super) const PART: &str = "part";

/// The space an item's own id is in.
pub(super) const ITEM_SPACE: &str = "metadata";

const ITEM_SEGMENT: &str = "(?P<item>[^/]+)";

const TOKEN_SHAPE: &str = "^[A-Za-z0-9_.-]{20,2048}$";

const PARTS: &str = "/library/parts";

const TRANSCODED: &str =
    "(?i)^/(?:video|music)/:/transcode/universal/(?:start|decision|subtitles)(?:\\.[a-z0-9]+)?$";

const TRANSCODED_FROM: &str = "path";

const PICTURED: &str = "(?i)^/photo/:/transcode$";

const PICTURED_FROM: &str = "url";

const PICTURES: &str = "(?:thumb|art|banner|clearLogo|theme)";

const REFUSED: [&str; 4] = [
    "(?i)^/library/streams(?:/|$)",
    "(?i)^/downloadQueue(?:/|$)",
    "(?i)^/sync(?:/|$)",
    "(?i)^/livetv(?:/|$)",
];

const LIVE_SESSIONS: &str = "/livetv/sessions/";

const LIVE_STREAM: &str = "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";

const TRANSCODED_LIVE: &str = "(?i)^/video/:/transcode/universal/start\\.m3u8$";

const TRANSCODED_LIVE_SEGMENTS: &str = "(?i)^/video/:/transcode/universal/session/";

const LIVE_CONSUMER: &str = "[0-9A-Za-z_-]{1,64}";

const LIVE_PLAYLIST: &str = "index\\.m3u8";

const LIVE_SEGMENT: &str = "[0-9A-Za-z_-]{1,64}\\.(?:ts|m4s|mp4|aac|vtt)";

const TRANSCODED_SEGMENT: &str =
    "[0-9a-z]{1,16}/(?:index\\.m3u8|header|[0-9]{1,10}\\.(?:ts|m4s|vtt))";

const LIVE_SESSION_PARAMETER: &str = "session";

const STREAMED: &str = "protocol=hls&X-Plex-Platform=Chrome";

/// Where the guard presents a member's token to Plex: in the query parameter Plex reads
/// it from.
#[must_use]
pub fn presented() -> Presentation {
    Presentation::Query {
        parameter: TOKEN.to_owned(),
    }
}

fn live_stream() -> String {
    format!("^{LIVE_STREAM}$")
}

fn live_session() -> String {
    format!("{LIVE_SESSIONS}(?P<stream>{LIVE_STREAM})/{LIVE_CONSUMER}/")
}

fn digits() -> String {
    format!("[0-9]{{1,{LONGEST_ID}}}")
}

fn item_digits() -> String {
    format!("(?P<item>{})", digits())
}

fn byte_path(pattern: String, query: Option<InQuery>, space: &str) -> BytePath {
    BytePath {
        pattern,
        query,
        space: Some(space.to_owned()),
        item: format!("^{}$", digits()),
        exempt: Vec::new(),
        naming: Vec::new(),
    }
}

fn declared() -> Declared {
    Declared {
        bytes: vec![
            byte_path(format!("(?i)^{PARTS}/{ITEM_SEGMENT}(?:/|$)"), None, PART),
            byte_path(
                format!("(?i)^{METADATA}/{ITEM_SEGMENT}(?:/|$)"),
                None,
                ITEM_SPACE,
            ),
            byte_path(
                TRANSCODED.to_owned(),
                Some(InQuery {
                    parameter: TRANSCODED_FROM.to_owned(),
                    pattern: format!("^{METADATA}/{}$", item_digits()),
                }),
                ITEM_SPACE,
            ),
            byte_path(
                PICTURED.to_owned(),
                Some(InQuery {
                    parameter: PICTURED_FROM.to_owned(),
                    pattern: format!(
                        "^(?:http://127\\.0\\.0\\.1:{PORT})?{METADATA}/{}/{PICTURES}(?:/[0-9]+)?$",
                        item_digits()
                    ),
                }),
                ITEM_SPACE,
            ),
        ],
        live: vec![
            LivePath {
                pattern: format!("(?i)^{}(?:{LIVE_PLAYLIST}|{LIVE_SEGMENT})$", live_session()),
                parameter: None,
                stream: live_stream(),
                bound: Vec::new(),
            },
            LivePath {
                pattern: format!(
                    "{TRANSCODED_LIVE_SEGMENTS}(?P<stream>{LIVE_STREAM})/{TRANSCODED_SEGMENT}$"
                ),
                parameter: Some(LIVE_SESSION_PARAMETER.to_owned()),
                stream: live_stream(),
                bound: Vec::new(),
            },
            LivePath {
                pattern: TRANSCODED_LIVE.to_owned(),
                parameter: Some(LIVE_SESSION_PARAMETER.to_owned()),
                stream: live_stream(),
                bound: vec![Bound {
                    parameter: TRANSCODED_FROM.to_owned(),
                    pattern: format!("^{}{LIVE_PLAYLIST}$", live_session()),
                }],
            },
        ],
        refused: REFUSED.map(str::to_owned).to_vec(),
        question: Some(Question {
            method: Method::Get,
            path: MAY.to_owned(),
            to: Addressee::Adapter,
            answers: Answers::from_adapter(),
        }),
        token: Token {
            presented: presented(),
            shape: TOKEN_SHAPE.to_owned(),
            parameters: vec![TOKEN.to_owned()],
            headers: vec![TOKEN.to_owned()],
        },
        locations: Locations {
            stream: format!(
                "{BASE}/video/:/transcode/universal/start.m3u8?path=%2Flibrary%2Fmetadata%2F{ITEM}&{STREAMED}"
            ),
            poster: format!("{BASE}{METADATA}/{ITEM}/thumb"),
            backdrop: format!("{BASE}{METADATA}/{ITEM}/art"),
            playlist: None,
        },
        ports: Ports {
            upstream: PORT,
            encrypted: None,
            internal_tls: None,
        },
    }
}

#[async_trait]
impl Declaring for Plex {
    async fn declared(&self) -> Result<Declared, Failure> {
        Ok(declared())
    }
}
