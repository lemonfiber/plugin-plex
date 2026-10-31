use axum::routing::get;
use axum::Router;
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Applied, Encrypting as _, Failure};

use super::super::encrypting::internal_tls;
use super::{asked, plex, Shared, TOKEN};

const PATH: &str = "/config/lemonfiber/internal.p12";

const PLAIN: &str = r#"{"MediaContainer":{"size":3,"Setting":[{"id":"customCertificatePath","value":""},{"id":"customCertificateKey","value":""},{"id":"FriendlyName","value":7}]}}"#;

const SERVING: &str = r#"{"MediaContainer":{"size":2,"Setting":[{"id":"customCertificatePath","value":"/config/lemonfiber/internal.p12"},{"id":"customCertificateKey","value":""}]}}"#;

const KEYED: &str = r#"{"MediaContainer":{"size":2,"Setting":[{"id":"customCertificatePath","value":"/config/lemonfiber/internal.p12"},{"id":"customCertificateKey","value":"secret"}]}}"#;

fn pin() -> String {
    "ab".repeat(32)
}

#[tokio::test]
async fn the_certificate_is_set_in_plexs_preferences_and_in_force_once_plex_restarts() {
    let shared = Shared::claimed("set");
    let fake = Fake::by_route_in_turn(vec![
        (Method::Put, "/:/prefs", vec![Answer::reply(200, "")]),
        (
            Method::Get,
            "/:/prefs",
            vec![Answer::reply(200, PLAIN), Answer::reply(200, SERVING)],
        ),
    ]);

    let applied = plex(&fake, &shared).serve_tls(PATH, &pin()).await;

    assert!(matches!(applied, Ok(Applied::OnRestart)));
    assert_eq!(
        asked(&fake),
        [
            (Method::Get, "/:/prefs".to_owned(), Some(TOKEN.to_owned())),
            (
                Method::Put,
                "/:/prefs?customCertificatePath=%2Fconfig%2Flemonfiber%2Finternal.p12&customCertificateKey="
                    .to_owned(),
                Some(TOKEN.to_owned())
            ),
            (Method::Get, "/:/prefs".to_owned(), Some(TOKEN.to_owned())),
        ]
    );
}

#[tokio::test]
async fn a_certificate_plex_already_presents_is_in_force_now_and_nothing_is_written() {
    let shared = Shared::claimed("presented");
    let served = lemonfiber_pinned::fake::tls(Router::new().route("/", get(|| async { "" })));
    assert!(served.is_some());
    let Some(served) = served else { return };
    let fake = Fake::by_path(vec![("/:/prefs", Answer::reply(200, SERVING))]);
    let plex = plex(&fake, &shared).with_internal_tls(served.base.clone());

    assert!(matches!(
        plex.serve_tls(PATH, &served.pin).await,
        Ok(Applied::Now)
    ));
    assert!(matches!(
        plex.serve_tls(PATH, &pin()).await,
        Ok(Applied::OnRestart)
    ));
    assert!(asked(&fake)
        .iter()
        .all(|(method, _, _)| *method == Method::Get));
}

#[tokio::test]
async fn a_preference_set_but_not_presented_yet_is_in_force_once_plex_restarts() {
    let shared = Shared::claimed("unpresented");
    let fake = Fake::by_path(vec![("/:/prefs", Answer::reply(200, SERVING))]);
    let unreachable = plex(&fake, &shared);
    assert!(matches!(
        unreachable.serve_tls(PATH, &pin()).await,
        Ok(Applied::OnRestart)
    ));
    let nowhere = super::super::Plex::new(
        fake.clone(),
        "not a url",
        shared.path(),
        std::sync::Arc::new(String::new()),
    );
    assert!(matches!(
        nowhere.serve_tls(PATH, &pin()).await,
        Ok(Applied::OnRestart)
    ));
}

#[tokio::test]
async fn a_preference_read_back_otherwise_or_a_refused_write_fails() {
    let shared = Shared::claimed("unset");
    for after in [KEYED, PLAIN] {
        let fake = Fake::by_route_in_turn(vec![
            (Method::Put, "/:/prefs", vec![Answer::reply(200, "")]),
            (
                Method::Get,
                "/:/prefs",
                vec![Answer::reply(200, PLAIN), Answer::reply(200, after)],
            ),
        ]);
        assert!(matches!(
            plex(&fake, &shared).serve_tls(PATH, &pin()).await,
            Err(Failure::Refused { detail, .. }) if detail.contains("reads its preferences back otherwise")
        ));
    }
    let refusing = Fake::by_route(vec![
        (Method::Put, "/:/prefs", Answer::reply(401, "")),
        (Method::Get, "/:/prefs", Answer::reply(200, PLAIN)),
    ]);
    assert!(plex(&refusing, &shared)
        .serve_tls(PATH, &pin())
        .await
        .is_err());
}

#[tokio::test]
async fn no_path_or_a_pin_that_is_no_sha256_is_refused_before_anything_is_asked() {
    let shared = Shared::claimed("refused");
    let fake = Fake::silent();
    let plex = plex(&fake, &shared);
    for (path, pin) in [
        (" ", pin()),
        (PATH, "AB".repeat(32)),
        (PATH, "ab".repeat(31)),
        (PATH, "zz".repeat(32)),
    ] {
        assert!(matches!(
            plex.serve_tls(path, &pin).await,
            Err(Failure::Refused { .. })
        ));
    }
    assert!(fake.requests().is_empty());
}

#[test]
fn the_internal_tls_port_is_plexs_own_port_on_its_own_host() {
    assert_eq!(
        internal_tls("http://plex:32400").map(|at| at.to_string()),
        Some("https://plex:32400/".to_owned())
    );
    assert_eq!(internal_tls("nothing"), None);
}
