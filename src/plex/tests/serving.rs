use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::{AppKeys as _, Failure, Fronted as _, Library as _, Upkeep as _};

use super::{asked, plex, Shared, TOKEN};

const FOUND: &str = r#"{"MediaContainer":{"size":1,"Metadata":[{"ratingKey":"2","type":"show","title":"Test Show"}]}}"#;

const NONE: &str = r#"{"MediaContainer":{"size":0}}"#;

const SERIES: &str = r#"{"MediaContainer":{"size":3,"Metadata":[{"ratingKey":"2","type":"show","title":"Test Show","childCount":2,"leafCount":9},{"ratingKey":"no","type":"show","title":"Odd"},{"ratingKey":"6","type":"show","title":"Unseasoned"}]}}"#;

fn unsupported<T>(answer: &Result<T, Failure>) -> bool {
    matches!(answer, Err(Failure::Unsupported { .. }))
}

#[tokio::test]
async fn a_title_is_looked_for_among_the_films_or_the_series_by_kind() {
    let shared = Shared::claimed("has-item");
    let fake = Fake::by_path(vec![
        ("type=2", Answer::reply(200, FOUND)),
        ("type=1", Answer::reply(200, NONE)),
    ]);
    let plex = plex(&fake, &shared);

    assert!(matches!(plex.has_item(Kind::Tv, "test sh").await, Ok(true)));
    assert!(matches!(
        plex.has_item(Kind::Movies, "test sh").await,
        Ok(false)
    ));
    assert_eq!(
        asked(&fake).first(),
        Some(&(
            Method::Get,
            "/library/all?type=2&title=test+sh&X-Plex-Container-Start=0&X-Plex-Container-Size=1"
                .to_owned(),
            Some(TOKEN.to_owned())
        ))
    );
}

#[tokio::test]
async fn a_rescan_asks_plex_to_look_over_every_library() {
    let shared = Shared::claimed("rescan");
    let fake = Fake::by_path(vec![(
        "/library/sections/all/refresh",
        Answer::reply(200, ""),
    )]);

    assert!(plex(&fake, &shared).rescan().await.is_ok());
    assert_eq!(
        asked(&fake),
        [(
            Method::Get,
            "/library/sections/all/refresh".to_owned(),
            Some(TOKEN.to_owned())
        )]
    );
    let refusing = Fake::always(Answer::reply(401, ""));
    assert!(plex(&refusing, &shared).rescan().await.is_err());
}

#[tokio::test]
async fn the_series_held_are_counted_by_seasons_and_episodes_bounded_by_plex() {
    let shared = Shared::claimed("series-held");
    let fake = Fake::by_path(vec![("/library/all", Answer::reply(200, SERIES))]);
    let plex = plex(&fake, &shared);

    let held: Vec<(String, String, u32, u32)> = plex
        .series_held(5)
        .await
        .map(|all| {
            all.into_iter()
                .map(|one| (one.id, one.title, one.seasons, one.episodes))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        held,
        [
            ("2".to_owned(), "Test Show".to_owned(), 2, 9),
            ("6".to_owned(), "Unseasoned".to_owned(), 0, 0),
        ]
    );
    assert!(matches!(plex.series_held(0).await, Ok(all) if all.is_empty()));
    assert_eq!(asked(&fake).len(), 1);
}

#[tokio::test]
async fn an_item_is_read_afresh_where_plex_holds_it() {
    let shared = Shared::claimed("refresh");
    let fake = Fake::by_route(vec![
        (
            Method::Put,
            "/library/metadata/2/refresh",
            Answer::reply(200, ""),
        ),
        (
            Method::Put,
            "/library/metadata/9/refresh",
            Answer::reply(404, ""),
        ),
        (
            Method::Put,
            "/library/metadata/8/refresh",
            Answer::reply(500, ""),
        ),
    ]);
    let plex = plex(&fake, &shared);

    assert!(plex.refresh("2").await.is_ok());
    for id in ["9", "8", "x/../2"] {
        assert!(matches!(
            plex.refresh(id).await,
            Err(Failure::Refused { .. })
        ));
    }
    assert_eq!(asked(&fake).len(), 3);
}

#[tokio::test]
async fn proxies_origins_restarts_and_keys_are_unsupported() {
    let shared = Shared::claimed("unsupported");
    let fake = Fake::silent();
    let plex = plex(&fake, &shared);

    assert!(unsupported(&plex.known_proxies().await));
    assert!(unsupported(&plex.trust_only("10.0.0.2").await));
    assert!(unsupported(&plex.allowed_origins().await));
    assert!(unsupported(&plex.allow_only("https://a.example").await));
    assert!(unsupported(&plex.restart().await));
    assert!(unsupported(&plex.filed_as("app").await));
    assert!(unsupported(&plex.mint("app").await));
    assert!(unsupported(&plex.dated("app").await));
    assert!(unsupported(&plex.answers_to("key").await));
    assert!(unsupported(&plex.revoke("key").await));
    assert!(fake.requests().is_empty());
}
