use lemonfiber_fixtures::http::Fake;
use lemonfiber_ports::service::{BytePath, Declared, Declaring as _, Locations};
use regex::Regex;

use super::{plex, Shared};

const HOUSEHOLD: &str = "https://plex.household.example";

const LIVE: &str = "dc30f95e-6379-44f7-8168-172ffc820496";

const OTHER_LIVE: &str = "1f2a6c1e-8b3d-4e5f-9a7b-1c2d3e4f5a6b";

async fn declared() -> Declared {
    let shared = Shared::holding("declared", None);
    let fake = Fake::silent();
    let declared = plex(&fake, &shared).declared().await;
    assert!(fake.requests().is_empty());
    declared.unwrap_or_else(|_| unreachable!("Plex's declaration is written, never asked"))
}

fn compiled(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|_| unreachable!("{pattern} compiles"))
}

/// The space and item the first byte path covering `path` with `query` names, or none.
fn covered(declared: &Declared, path: &str, query: &[(&str, &str)]) -> Option<(String, String)> {
    let path_of = |byte: &BytePath| {
        compiled(&byte.pattern)
            .captures(path)
            .map(|found| found.name("item").map(|item| item.as_str().to_owned()))
    };
    declared.bytes.iter().find_map(|byte| {
        let in_path = path_of(byte)?;
        let item = match &byte.query {
            None => in_path?,
            Some(held) => {
                let value = query.iter().find(|(name, _)| *name == held.parameter)?.1;
                compiled(&held.pattern)
                    .captures(value)?
                    .name("item")?
                    .as_str()
                    .to_owned()
            }
        };
        compiled(&byte.item)
            .is_match(&item)
            .then(|| (byte.space.clone().unwrap_or_default(), item))
    })
}

fn named(space: &str, item: &str) -> (String, String) {
    (space.to_owned(), item.to_owned())
}

#[tokio::test]
async fn the_declaration_is_one_a_guard_can_enforce() {
    assert!(declared().await.checked().is_ok());
}

#[tokio::test]
async fn a_part_file_and_its_index_are_asked_about_by_the_part() {
    let declared = declared().await;
    assert_eq!(
        covered(&declared, "/library/parts/2/1793462557/file.mp4", &[]),
        Some(named("part", "2"))
    );
    assert_eq!(
        covered(&declared, "/library/parts/2/indexes/sd/0", &[]),
        Some(named("part", "2"))
    );
    assert_eq!(
        covered(&declared, "/Library/Parts/2/1793462557/file.mp4", &[]),
        Some(named("part", "2"))
    );
}

#[tokio::test]
async fn an_items_pictures_and_its_own_answer_are_asked_about_by_the_item() {
    let declared = declared().await;
    for path in [
        "/library/metadata/1",
        "/library/metadata/1/thumb",
        "/library/metadata/1/thumb/1793462648",
        "/library/metadata/1/art/1793462648",
        "/library/metadata/1/theme/1793462648",
    ] {
        assert_eq!(
            covered(&declared, path, &[]),
            Some(named("metadata", "1")),
            "{path}"
        );
    }
}

#[tokio::test]
async fn a_transcode_is_asked_about_by_the_item_its_path_names() {
    let declared = declared().await;
    for path in [
        "/video/:/transcode/universal/start.m3u8",
        "/video/:/transcode/universal/start",
        "/video/:/transcode/universal/decision",
        "/video/:/transcode/universal/subtitles",
        "/music/:/transcode/universal/start.mp3",
    ] {
        assert_eq!(
            covered(&declared, path, &[("path", "/library/metadata/4")]),
            Some(named("metadata", "4")),
            "{path}"
        );
    }
}

#[tokio::test]
async fn a_transcode_naming_no_item_of_plexs_is_covered_by_nothing_that_could_pass_it() {
    let declared = declared().await;
    let start = "/video/:/transcode/universal/start.m3u8";
    assert_eq!(covered(&declared, start, &[]), None);
    assert_eq!(
        covered(&declared, start, &[("path", "/library/parts/2/file.mp4")]),
        None
    );
    assert_eq!(
        covered(&declared, start, &[("path", "/library/metadata/4/../9")]),
        None
    );
    assert!(declared
        .bytes
        .iter()
        .any(|byte| compiled(&byte.pattern).is_match(start)));
}

#[tokio::test]
async fn a_picture_resized_is_asked_about_only_where_it_is_an_items_own() {
    let declared = declared().await;
    let resized = "/photo/:/transcode";
    for url in [
        "/library/metadata/1/thumb",
        "/library/metadata/1/thumb/1793462648",
        "http://127.0.0.1:32400/library/metadata/1/art/1793462648",
    ] {
        assert_eq!(
            covered(&declared, resized, &[("url", url), ("width", "100")]),
            Some(named("metadata", "1")),
            "{url}"
        );
    }
    for url in [
        "https://example.com/x.jpg",
        "http://10.0.0.2:32400/library/metadata/1/thumb",
        "/library/metadata/1/thumb?url=https://example.com/x.jpg",
    ] {
        assert_eq!(covered(&declared, resized, &[("url", url)]), None, "{url}");
    }
}

#[tokio::test]
async fn a_query_value_carrying_anything_past_the_item_is_refused_by_its_own_pattern() {
    let declared = declared().await;
    let transcoded = "/video/:/transcode/universal/start.m3u8";
    for path in [
        "/library/metadata/4/children",
        "/library/metadata/4?url=https://example.com/",
        "/library/metadata/4#fragment",
        "/library/metadata/4/../9",
        "/library/metadata/../4",
        "http://10.0.0.2:32400/library/metadata/4",
        "/library/metadata/4a",
        "/library/metadata/123456789012345678901",
        "x/library/metadata/4",
    ] {
        assert_eq!(
            covered(&declared, transcoded, &[("path", path)]),
            None,
            "{path}"
        );
    }
    let resized = "/photo/:/transcode";
    for url in [
        "/library/metadata/1/thumb/1793462648/more",
        "/library/metadata/1/thumb?url=https://example.com/x.jpg",
        "/library/metadata/1/thumb#fragment",
        "/library/metadata/1/../2/thumb",
        "http://127.0.0.1:32400.example.com/library/metadata/1/thumb",
        "http://plex:32400/library/metadata/1/thumb",
        "https://127.0.0.1:32400/library/metadata/1/thumb",
        "x/library/metadata/1/thumb",
    ] {
        assert_eq!(covered(&declared, resized, &[("url", url)]), None, "{url}");
    }
}

#[tokio::test]
async fn a_transcode_sessions_segments_are_no_byte_path_of_an_item() {
    let declared = declared().await;
    assert_eq!(
        covered(
            &declared,
            "/video/:/transcode/universal/session/bfb15d67-b7db-432b-8ecd-16ec7a10951c/base/index.m3u8",
            &[]
        ),
        None
    );
}

#[tokio::test]
async fn bytes_served_under_ids_no_question_takes_are_refused() {
    let declared = declared().await;
    let refused = |path: &str| {
        declared
            .refused
            .iter()
            .any(|pattern| compiled(pattern).is_match(path))
    };
    for path in [
        "/library/streams/7",
        "/library/streams/7.srt",
        "/downloadQueue/1/item/2/media",
        "/sync/items/3",
        "/livetv",
        "/livetv/",
        "/livetv/dvrs/6/channels/5.1/tune",
        "/LiveTV/epg/channels",
        "/livetv/sessions",
        "/livetv/sessions/abc",
        "/LiveTV/sessions/abc/",
        &format!("/livetv/sessions/{LIVE}/lemonfiber/index.m3u8"),
    ] {
        assert!(refused(path), "{path}");
    }
    for path in ["/library/sections", "/library/metadata/1", "/identity"] {
        assert!(!refused(path), "{path}");
    }
}

#[tokio::test]
async fn under_livetv_only_what_a_live_stream_path_covers_end_to_end_goes_on() {
    let declared = declared().await;
    let decided = |path: &str| {
        if declared
            .live
            .iter()
            .any(|live| compiled(&live.pattern).is_match(path))
        {
            Some(path.to_owned())
        } else if declared
            .refused
            .iter()
            .any(|pattern| compiled(pattern).is_match(path))
        {
            None
        } else {
            Some(path.to_owned())
        }
    };
    let session = format!("/livetv/sessions/{LIVE}/lemonfiber");
    for path in [
        format!("{session}/index.m3u8"),
        format!("{session}/00001.ts"),
    ] {
        assert_eq!(decided(&path), Some(path.clone()), "{path}");
    }
    for path in [
        format!("{session}/extra.bin"),
        format!("{session}/index.m3u8/more"),
        format!("{session}/deeper/00001.ts"),
        format!("{session}/"),
        format!("/livetv/sessions/{LIVE}"),
        format!("/livetv/sessions/{LIVE}/index.m3u8"),
        "/livetv/sessions/not-a-session/lemonfiber/index.m3u8".to_owned(),
        "/livetv/dvrs/6/channels/5.1/tune".to_owned(),
    ] {
        assert_eq!(decided(&path), None, "{path}");
    }
}

#[tokio::test]
async fn an_item_streams_from_a_transcode_and_shows_its_own_pictures() {
    let located = |template: &str| Locations::located(template, HOUSEHOLD, "4");
    let declared = declared().await;
    assert_eq!(
        located(&declared.locations.stream).as_deref(),
        Some("https://plex.household.example/video/:/transcode/universal/start.m3u8?path=%2Flibrary%2Fmetadata%2F4&protocol=hls&X-Plex-Platform=Chrome")
    );
    assert_eq!(
        located(&declared.locations.poster).as_deref(),
        Some("https://plex.household.example/library/metadata/4/thumb")
    );
    assert_eq!(
        located(&declared.locations.backdrop).as_deref(),
        Some("https://plex.household.example/library/metadata/4/art")
    );
}

#[tokio::test]
async fn a_members_token_reaches_plex_in_the_parameter_and_header_plex_reads() {
    let token = declared().await.token;
    assert_eq!(
        token.presented,
        lemonfiber_ports::service::Presentation::Query {
            parameter: "X-Plex-Token".to_owned()
        }
    );
    assert_eq!(token.parameters, ["X-Plex-Token"]);
    assert_eq!(token.headers, ["X-Plex-Token"]);
    let shape = compiled(&token.shape);
    assert!(shape.is_match("abcDEF0123456789_-xy"));
    assert!(!shape.is_match("short"));
    assert!(!shape.is_match("abcDEF0123456789_-xy z"));
}

/// The live stream a request on `path` with the decoded query `query` names under
/// ARCH-R417, read against the first live-stream path covering it: from its group or its
/// parameter, carried once under that spelling, every bound parameter and, beside a
/// parameter, the path's own group naming the same; none where it names none or two.
fn streamed(declared: &Declared, path: &str, query: &[(&str, &str)]) -> Option<String> {
    let once = |name: &str| {
        let carried: Vec<&(&str, &str)> = query
            .iter()
            .filter(|(carried, _)| carried.eq_ignore_ascii_case(name))
            .collect();
        match carried.as_slice() {
            [(spelled, value)] if *spelled == name => Some((*value).to_owned()),
            _ => None,
        }
    };
    let live = declared.live.iter().find(|live| {
        compiled(&live.pattern).is_match(path)
            && live.parameter.as_deref().is_none_or(|parameter| {
                query
                    .iter()
                    .any(|(carried, _)| carried.eq_ignore_ascii_case(parameter))
            })
    })?;
    let in_path = compiled(&live.pattern).captures(path).and_then(|found| {
        found
            .name("stream")
            .map(|stream| stream.as_str().to_owned())
    });
    let stream = match &live.parameter {
        None => in_path?,
        Some(parameter) => {
            let carried = once(parameter)?;
            if in_path.is_some_and(|named| named != carried) {
                return None;
            }
            carried
        }
    };
    let agreed = live.bound.iter().all(|bound| {
        once(&bound.parameter).is_some_and(|value| {
            compiled(&bound.pattern)
                .captures(&value)
                .and_then(|found| found.name("stream"))
                .is_some_and(|named| named.as_str() == stream)
        })
    });
    (agreed && compiled(&live.stream).is_match(&stream)).then_some(stream)
}

#[tokio::test]
async fn a_transcode_of_a_live_session_is_passed_only_where_its_path_reads_the_session_it_serves() {
    let declared = declared().await;
    let start = "/video/:/transcode/universal/start.m3u8";
    let reading = |stream: &str| format!("/livetv/sessions/{stream}/lemonfiber/index.m3u8");
    let same = reading(LIVE);
    let other = reading(OTHER_LIVE);

    assert_eq!(
        streamed(
            &declared,
            start,
            &[("session", LIVE), ("path", &same), ("protocol", "hls")]
        ),
        Some(LIVE.to_owned())
    );
    for query in [
        vec![("session", LIVE), ("path", other.as_str())],
        vec![("session", LIVE), ("path", "/library/metadata/5")],
        vec![("session", LIVE)],
        vec![("session", LIVE), ("path", &same), ("path", &same)],
        vec![("session", LIVE), ("Path", &same)],
        vec![("Session", LIVE), ("path", &same)],
        vec![("session", "not-a-session"), ("path", &same)],
    ] {
        assert_eq!(streamed(&declared, start, &query), None, "{query:?}");
    }
}

#[tokio::test]
async fn a_live_transcodes_segments_are_passed_only_where_their_path_and_session_name_one_live_stream(
) {
    let declared = declared().await;
    let segment =
        |stream: &str| format!("/video/:/transcode/universal/session/{stream}/base/00001.ts");

    assert_eq!(
        streamed(&declared, &segment(LIVE), &[("session", LIVE)]),
        Some(LIVE.to_owned())
    );
    assert_eq!(
        streamed(&declared, &segment(OTHER_LIVE), &[("session", LIVE)]),
        None
    );
    assert_eq!(
        streamed(
            &declared,
            &format!("/livetv/sessions/{LIVE}/lemonfiber/00001.ts"),
            &[]
        ),
        Some(LIVE.to_owned())
    );
}

#[tokio::test]
async fn an_item_transcodes_segments_carrying_no_session_are_no_live_stream_and_no_items_byte_path()
{
    let declared = declared().await;
    let segment =
        "/video/:/transcode/universal/session/bfb15d67-b7db-432b-8ecd-16ec7a10951c/base/00001.ts";

    assert_eq!(streamed(&declared, segment, &[]), None);
    assert_eq!(covered(&declared, segment, &[]), None);
}

#[tokio::test]
async fn every_live_stream_path_ends_at_a_playlist_or_segment_file() {
    let declared = declared().await;
    let covers = |path: &str| {
        declared
            .live
            .iter()
            .any(|live| compiled(&live.pattern).is_match(path))
    };
    let session = format!("/livetv/sessions/{LIVE}/lemonfiber");
    let transcoded = format!("/video/:/transcode/universal/session/{LIVE}/base");

    for path in [
        format!("{session}/index.m3u8"),
        format!("{session}/00001.ts"),
        format!("{transcoded}/index.m3u8"),
        format!("{transcoded}/00001.ts"),
        "/video/:/transcode/universal/start.m3u8".to_owned(),
    ] {
        assert!(covers(&path), "{path}");
    }
    for path in [
        format!("{session}/index.m3u8/more"),
        format!("{session}/deeper/00001.ts"),
        format!("{session}/"),
        format!("{transcoded}/index.m3u8/more"),
        format!("{transcoded}/x/y/00001.ts"),
        "/video/:/transcode/universal/start.m3u8/more".to_owned(),
        format!("/livetv/sessions/{LIVE}/index.m3u8"),
    ] {
        assert!(!covers(&path), "{path}");
    }
    assert!(declared.live.iter().all(|live| live.pattern.ends_with('$')
        && live.bound.iter().all(|bound| bound.pattern.ends_with('$'))));
}
