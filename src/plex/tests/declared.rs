use lemonfiber_fixtures::http::Fake;
use lemonfiber_ports::service::{BytePath, Declared, Declaring as _, Locations};
use regex::Regex;

use super::{plex, Shared};

const HOUSEHOLD: &str = "https://plex.household.example";

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
        "/livetv/sessions/abc/index.m3u8",
        "/LiveTV/sessions/abc/index.m3u8",
    ] {
        assert!(refused(path), "{path}");
    }
    for path in ["/library/sections", "/library/metadata/1", "/identity"] {
        assert!(!refused(path), "{path}");
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
