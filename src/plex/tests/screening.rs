use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, HowFar, Medium, Picture, Screening as _};

use super::{asked, plex, Shared, TOKEN};

const FILM: &str = r#"{"MediaContainer":{"size":1,"Metadata":[{"ratingKey":"1","type":"movie","title":"Big Test","year":2020,"summary":"A test.","duration":5400000,"contentRating":"nl/12","originallyAvailableAt":"2020-05-01","thumb":"/library/metadata/1/thumb/1","art":"/library/metadata/1/art/1","Genre":[{"tag":"Drama"},{"tag":"Comedy"}]}]}}"#;

const SERIES: &str = r#"{"MediaContainer":{"size":1,"Metadata":[{"ratingKey":"2","type":"show","title":"Test Show","summary":"  ","childCount":1,"leafCount":2,"thumb":"/library/metadata/2/thumb/1"}]}}"#;

const SEASONS: &str = r#"{"MediaContainer":{"size":2,"Metadata":[{"ratingKey":"3","type":"season","title":"Season 1","index":1},{"ratingKey":"bad","type":"season","title":"Elsewhere"}]}}"#;

const EPISODES: &str = r#"{"MediaContainer":{"size":2,"Metadata":[{"ratingKey":"4","type":"episode","title":"Pilot","index":1,"duration":5000,"summary":"It starts."},{"ratingKey":"5","type":"episode","title":"Second","index":2}]}}"#;

const HELD: &str = r#"{"MediaContainer":{"size":3,"Metadata":[{"ratingKey":"1","type":"movie","title":"Big Test","year":2020,"thumb":"/t"},{"ratingKey":"x","type":"movie","title":"Odd"},{"ratingKey":"2","type":"show","title":"Test Show","art":"/a"}]}}"#;

const SESSIONS: &str = r#"{"MediaContainer":{"size":3,"Metadata":[{"ratingKey":"4","type":"episode","title":"Pilot","index":1,"parentIndex":1,"grandparentTitle":"Test Show","User":{"id":"7","title":"Ann"},"Player":{"title":"Living room","state":"paused"}},{"ratingKey":"1","type":"movie","title":"Big Test","index":3,"grandparentTitle":"Nothing","User":{"id":"8","title":"Bo"},"Player":{"title":"Phone","state":"playing"}},{"ratingKey":"9","type":"movie","title":"Unwatched"}]}}"#;

/// An episode's title, number, minutes and overview.
type Episode = (String, Option<u32>, Option<u32>, Option<String>);

#[tokio::test]
async fn a_film_is_read_whole_with_its_certificate_freed_of_its_country() {
    let shared = Shared::claimed("film");
    let fake = Fake::by_path(vec![("/library/metadata/1", Answer::reply(200, FILM))]);

    let detail = plex(&fake, &shared).title(None, "1").await;

    assert!(matches!(detail, Ok(Some(_))), "{detail:?}");
    let Ok(Some(detail)) = detail else { return };
    assert_eq!(detail.item.title, "Big Test");
    assert_eq!(detail.item.year, Some(2020));
    assert_eq!(detail.item.medium, Medium::Film);
    assert!(detail.item.holds.poster && detail.item.holds.backdrop && detail.item.holds.plays);
    assert_eq!(detail.overview.as_deref(), Some("A test."));
    assert_eq!(detail.minutes, Some(90));
    assert_eq!(detail.certificate.as_deref(), Some("12"));
    assert_eq!(detail.released.as_deref(), Some("2020-05-01"));
    assert_eq!(detail.genres, ["Drama", "Comedy"]);
    assert!(detail.seasons.is_empty());
    assert_eq!(
        asked(&fake),
        [(
            Method::Get,
            "/library/metadata/1".to_owned(),
            Some(TOKEN.to_owned())
        )]
    );
}

#[tokio::test]
async fn a_series_is_read_with_each_season_and_its_episodes_in_order() {
    let shared = Shared::claimed("series");
    let fake = Fake::by_path(vec![
        ("/library/metadata/2/children", Answer::reply(200, SEASONS)),
        ("/library/metadata/3/children", Answer::reply(200, EPISODES)),
        ("/library/metadata/2", Answer::reply(200, SERIES)),
    ]);

    let detail = plex(&fake, &shared).title(None, "2").await;

    assert!(matches!(detail, Ok(Some(_))), "{detail:?}");
    let Ok(Some(detail)) = detail else { return };
    assert_eq!(detail.item.medium, Medium::Series);
    assert!(!detail.item.holds.plays && !detail.item.holds.backdrop);
    assert_eq!(detail.overview, None);
    assert_eq!(detail.certificate, None);
    assert_eq!(detail.seasons.len(), 1);
    let season = detail.seasons.first();
    assert_eq!(
        season.map(|one| (one.id.as_str(), one.number)),
        Some(("3", Some(1)))
    );
    let episodes: Vec<Episode> = season
        .map(|one| {
            one.episodes
                .iter()
                .map(|episode| {
                    (
                        episode.item.title.clone(),
                        episode.number,
                        episode.minutes,
                        episode.overview.clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        episodes,
        [
            (
                "Pilot".to_owned(),
                Some(1),
                Some(1),
                Some("It starts.".to_owned())
            ),
            ("Second".to_owned(), Some(2), None, None),
        ]
    );
}

#[tokio::test]
async fn an_id_plex_holds_nothing_under_or_that_is_no_id_is_nothing_asked_for() {
    let shared = Shared::claimed("absent");
    let fake = Fake::by_path(vec![
        ("/library/metadata/77", Answer::reply(404, "")),
        (
            "/library/metadata/78",
            Answer::reply(200, r#"{"MediaContainer":{"size":0}}"#),
        ),
    ]);
    let plex = plex(&fake, &shared);

    assert!(matches!(plex.title(None, "77").await, Ok(None)));
    assert!(matches!(plex.title(None, "78").await, Ok(None)));
    assert!(matches!(plex.title(None, "../1").await, Ok(None)));
    assert!(matches!(
        plex.picture(None, "1/x", Picture::Poster).await,
        Ok(None)
    ));
    assert_eq!(asked(&fake).len(), 2);
}

#[tokio::test]
async fn a_picture_is_kept_only_as_a_raster_image_its_bytes_agree_with() {
    let shared = Shared::claimed("pictures");
    let gif = "GIF89a\u{1}\u{0}\u{1}\u{0}";
    let fake = Fake::by_path(vec![
        (
            "/library/metadata/1/thumb",
            Answer::served(200, "image/gif; charset=binary", gif),
        ),
        (
            "/library/metadata/1/art",
            Answer::served(200, "image/svg+xml", "<svg/>"),
        ),
        ("/library/metadata/2/thumb", Answer::reply(404, "")),
        ("/library/metadata/3/thumb", Answer::reply(500, "")),
    ]);
    let plex = plex(&fake, &shared);

    let poster = plex.picture(None, "1", Picture::Poster).await;
    assert!(matches!(&poster, Ok(Some(image)) if image.media_type == "image/gif"));
    assert!(matches!(
        plex.picture(None, "1", Picture::Backdrop).await,
        Ok(None)
    ));
    assert!(matches!(
        plex.picture(None, "2", Picture::Poster).await,
        Ok(None)
    ));
    assert!(matches!(
        plex.picture(None, "3", Picture::Poster).await,
        Err(Failure::Refused { .. })
    ));
    let gone = Fake::silent();
    assert!(matches!(
        super::plex(&gone, &shared)
            .picture(None, "1", Picture::Poster)
            .await,
        Err(Failure::Unavailable { .. })
    ));
}

#[tokio::test]
async fn holdings_are_films_and_series_sorted_and_bounded_by_plex() {
    let shared = Shared::claimed("holdings");
    let fake = Fake::by_path(vec![("/library/all", Answer::reply(200, HELD))]);
    let plex = plex(&fake, &shared);

    let held = plex.holdings(None, 2).await;
    let titles: Vec<(String, Medium)> = held
        .map(|items| {
            items
                .into_iter()
                .map(|one| (one.title, one.medium))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        titles,
        [
            ("Big Test".to_owned(), Medium::Film),
            ("Test Show".to_owned(), Medium::Series)
        ]
    );
    assert!(matches!(plex.holdings(None, 0).await, Ok(items) if items.is_empty()));
    assert_eq!(
        asked(&fake),
        [(
            Method::Get,
            "/library/all?type=1%2C2&sort=titleSort&X-Plex-Container-Start=0&X-Plex-Container-Size=2"
                .to_owned(),
            Some(TOKEN.to_owned())
        )]
    );
}

#[tokio::test]
async fn what_plays_is_every_session_or_one_members_with_an_episode_named_by_its_series() {
    let shared = Shared::claimed("playing");
    let fake = Fake::by_path(vec![("/status/sessions", Answer::reply(200, SESSIONS))]);
    let plex = plex(&fake, &shared);

    let playing = plex.playing(None).await;
    assert!(playing.is_ok());
    let Ok(playing) = playing else { return };
    assert_eq!(playing.len(), 2);
    let episode = playing.first();
    assert_eq!(
        episode.map(|one| (
            one.member_id.as_str(),
            one.member.as_str(),
            one.title.as_str(),
            one.series.as_deref(),
            one.season,
            one.episode,
            one.paused,
            one.device.as_str(),
            one.medium
        )),
        Some((
            "7",
            "Ann",
            "Pilot",
            Some("Test Show"),
            Some(1),
            Some(1),
            true,
            "Living room",
            Medium::Episode
        ))
    );
    let film = playing.get(1);
    assert_eq!(
        film.map(|one| (one.series.clone(), one.episode, one.paused)),
        Some((None, None, false))
    );
    assert!(matches!(plex.playing(Some("8")).await, Ok(bo) if bo.len() == 1));
}

#[tokio::test]
async fn everything_asked_for_a_member_or_a_device_is_unsupported() {
    let shared = Shared::claimed("members");
    let fake = Fake::silent();
    let plex = plex(&fake, &shared);
    let unsupported = |failure: &Failure| matches!(failure, Failure::Unsupported { .. });

    assert!(plex
        .signed_in("7", "tv")
        .await
        .is_err_and(|why| unsupported(&why)));
    assert!(plex
        .title(Some("7"), "1")
        .await
        .is_err_and(|why| unsupported(&why)));
    assert!(plex
        .part_way("7", 5)
        .await
        .is_err_and(|why| unsupported(&why)));
    let how_far = HowFar {
        position: 1,
        ended: false,
    };
    assert!(plex
        .progressed("7", "1", &how_far)
        .await
        .is_err_and(|why| unsupported(&why)));
    assert!(plex
        .sign_out("tv")
        .await
        .is_err_and(|why| unsupported(&why)));
    assert!(plex
        .picture(Some("7"), "1", Picture::Poster)
        .await
        .is_err_and(|why| unsupported(&why)));
    assert!(plex
        .holdings(Some("7"), 5)
        .await
        .is_err_and(|why| unsupported(&why)));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn nothing_is_asked_of_plex_before_it_keeps_a_token() {
    let shared = Shared::holding("unclaimed", None);
    let fake = Fake::silent();
    let plex = plex(&fake, &shared);

    assert!(matches!(
        plex.holdings(None, 3).await,
        Err(Failure::Unauthorised { .. })
    ));
    assert!(matches!(
        plex.picture(None, "1", Picture::Poster).await,
        Err(Failure::Unauthorised { .. })
    ));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn an_item_neither_film_series_nor_episode_is_another_kind_and_a_track_plays() {
    let shared = Shared::claimed("other");
    let fake = Fake::by_path(vec![
        (
            "/library/metadata/10",
            Answer::reply(
                200,
                r#"{"MediaContainer":{"Metadata":[{"ratingKey":"10","type":"album","title":"An Album"}]}}"#,
            ),
        ),
        (
            "/library/metadata/11",
            Answer::reply(
                200,
                r#"{"MediaContainer":{"Metadata":[{"ratingKey":"11","type":"track","title":"A Track"}]}}"#,
            ),
        ),
    ]);
    let plex = plex(&fake, &shared);

    assert!(matches!(
        plex.title(None, "10").await,
        Ok(Some(detail)) if detail.item.medium == Medium::Other && !detail.item.holds.plays
    ));
    assert!(matches!(
        plex.title(None, "11").await,
        Ok(Some(detail)) if detail.item.medium == Medium::Other && detail.item.holds.plays
    ));
}
