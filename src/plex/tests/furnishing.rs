use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, FiledAt, Furnished, Furnishing as _};

use super::{asked, plex, Shared, TOKEN};

const NONE_HELD: &str = r#"{"MediaContainer":{"size":0}}"#;

const FILMS_HELD: &str = r#"{"MediaContainer":{"size":1,"Directory":[{"key":"1","type":"movie","title":"Films","Location":[{"id":1,"path":"/data/media/movies/"}]}]}}"#;

fn filed(media_type: &str) -> FiledAt {
    FiledAt {
        media_type: media_type.to_owned(),
        folder: format!("/data/media/{media_type}"),
    }
}

fn held(media_type: &str, held: bool) -> Furnished {
    Furnished {
        media_type: media_type.to_owned(),
        held,
    }
}

/// Each library Plex was asked to add, by the query it was asked with.
fn added(fake: &Fake) -> Vec<String> {
    asked(fake)
        .into_iter()
        .filter(|(method, _, _)| *method == Method::Post)
        .map(|(_, path, token)| {
            assert_eq!(token.as_deref(), Some(TOKEN));
            path
        })
        .collect()
}

#[tokio::test]
async fn a_library_is_added_for_each_media_type_plex_takes_and_none_for_another() {
    let shared = Shared::claimed("furnish-added");
    let fake = Fake::by_route(vec![
        (
            Method::Get,
            "/library/sections",
            Answer::reply(200, NONE_HELD),
        ),
        (Method::Post, "/library/sections?", Answer::reply(201, "")),
    ]);
    let plex = plex(&fake, &shared);

    let answer = plex
        .furnish(&[
            filed("movies"),
            filed("tv"),
            filed("music"),
            filed("comics"),
        ])
        .await;

    assert_eq!(
        answer.ok(),
        Some(vec![
            held("movies", true),
            held("tv", true),
            held("music", true),
            held("comics", false),
        ])
    );
    assert_eq!(
        added(&fake),
        [
            "/library/sections?name=Movies&type=movie&agent=tv.plex.agents.movie&scanner=Plex+Movie&language=en-US&location=%2Fdata%2Fmedia%2Fmovies",
            "/library/sections?name=TV&type=show&agent=tv.plex.agents.series&scanner=Plex+TV+Series&language=en-US&location=%2Fdata%2Fmedia%2Ftv",
            "/library/sections?name=Music&type=artist&agent=tv.plex.agents.music&scanner=Plex+Music&language=en-US&location=%2Fdata%2Fmedia%2Fmusic",
        ]
    );
}

#[tokio::test]
async fn a_folder_a_library_already_reads_is_held_and_nothing_is_added_or_changed() {
    let shared = Shared::claimed("furnish-held");
    let fake = Fake::by_route(vec![
        (
            Method::Get,
            "/library/sections",
            Answer::reply(200, FILMS_HELD),
        ),
        (Method::Post, "/library/sections?", Answer::reply(201, "")),
    ]);
    let plex = plex(&fake, &shared);

    let answer = plex.furnish(&[filed("movies")]).await;

    assert_eq!(answer.ok(), Some(vec![held("movies", true)]));
    assert!(added(&fake).is_empty());
    assert!(asked(&fake)
        .iter()
        .all(|(method, _, _)| *method == Method::Get));
}

#[tokio::test]
async fn a_folder_plex_refuses_is_answered_not_held() {
    let shared = Shared::claimed("furnish-refused");
    let fake = Fake::by_route(vec![
        (
            Method::Get,
            "/library/sections",
            Answer::reply(200, NONE_HELD),
        ),
        (Method::Post, "/library/sections?", Answer::reply(400, "")),
    ]);
    let plex = plex(&fake, &shared);

    let answer = plex.furnish(&[filed("tv")]).await;

    assert_eq!(answer.ok(), Some(vec![held("tv", false)]));
}

#[tokio::test]
async fn a_plex_failing_to_add_a_library_is_a_failure_not_an_answer() {
    let shared = Shared::claimed("furnish-failed");
    let fake = Fake::by_route(vec![
        (
            Method::Get,
            "/library/sections",
            Answer::reply(200, NONE_HELD),
        ),
        (Method::Post, "/library/sections?", Answer::reply(500, "")),
    ]);
    let plex = plex(&fake, &shared);

    assert!(plex.furnish(&[filed("tv")]).await.is_err());
}

#[tokio::test]
async fn a_plex_nobody_has_claimed_is_given_nothing() {
    let shared = Shared::holding("furnish-unclaimed", None);
    let fake = Fake::always(Answer::reply(200, NONE_HELD));
    let plex = plex(&fake, &shared);

    assert!(matches!(
        plex.furnish(&[filed("tv")]).await,
        Err(Failure::Unauthorised { .. })
    ));
    assert!(fake.requests().is_empty());
}
