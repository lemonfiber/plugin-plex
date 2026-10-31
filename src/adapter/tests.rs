use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lemonfiber_contract::adapter::{about_path, ready_path, About};
use lemonfiber_fixtures::http::{Answer, Fake};
use tower::ServiceExt as _;

use super::{about, kit, ADDRESS, DIGEST, RELEASE, UPSTREAM};
use crate::plex::Plex;

fn a_key() -> String {
    ('e'..='t').collect()
}

/// A configuration directory Plex has written nothing into yet.
fn unwritten() -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("plex-adapter-kit-routes-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&directory);
    directory
}

async fn answered_by(
    fake: &Arc<Fake>,
    method: &str,
    path: &str,
    key: Option<&str>,
    body: &str,
) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(key) = key {
        request = request.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    let Ok(request) = request.body(Body::from(body.to_owned())) else {
        return (StatusCode::IM_A_TEAPOT, Vec::new());
    };
    let plex = Arc::new(Plex::new(
        fake.clone(),
        ADDRESS,
        &unwritten(),
        Arc::new(String::new()),
    ));
    let Ok(response) = kit(a_key(), plex).router().oneshot(request).await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .map(|bytes| bytes.to_vec())
        .unwrap_or_default();
    (status, bytes)
}

fn kind_of(body: &[u8]) -> Option<serde_json::Value> {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|refusal| refusal.get("type").cloned())
}

#[test]
fn it_speaks_the_claim_and_the_media_server_contracts_and_names_the_release_its_recordings_were_taken_from(
) {
    let said = about();
    assert_eq!(said.speaks, ["owner.claim@1", "media.serve@1"]);
    assert_eq!(said.upstream, UPSTREAM);
    assert_eq!(
        said.releases
            .iter()
            .map(|release| (release.version.as_str(), release.digest.as_str()))
            .collect::<Vec<_>>(),
        [(RELEASE, DIGEST)]
    );
}

#[tokio::test]
async fn about_is_answered_under_the_plugin_s_key() {
    let key = a_key();
    let (status, body) = answered_by(&Fake::silent(), "GET", &about_path(), Some(&key), "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_slice::<About>(&body).ok(), Some(about()));
}

#[tokio::test]
async fn nothing_is_answered_without_the_key_or_under_another() {
    for key in [None, Some("not-the-plugin-s-key")] {
        for (method, path) in [
            ("POST", "/lemonfiber/owner.claim/v1/claim".to_owned()),
            ("POST", "/lemonfiber/media.serve/v1/rescan".to_owned()),
            ("GET", about_path()),
        ] {
            let fake = Fake::silent();
            assert_eq!(
                answered_by(&fake, method, &path, key, "{}").await.0,
                StatusCode::UNAUTHORIZED
            );
            assert!(fake.requests().is_empty());
        }
    }
}

#[tokio::test]
async fn an_unreachable_plex_is_settling_and_one_keeping_no_token_is_asked_nothing_else() {
    let key = a_key();
    let (status, body) = answered_by(
        &Fake::silent(),
        "POST",
        "/lemonfiber/owner.claim/v1/owner",
        Some(&key),
        "{}",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).ok(),
        Some(serde_json::json!("settling"))
    );
    let fake = Fake::silent();
    let (_, body) = answered_by(
        &fake,
        "POST",
        "/lemonfiber/media.serve/v1/rescan",
        Some(&key),
        "{}",
    )
    .await;
    assert_eq!(kind_of(&body), Some(serde_json::json!("unauthorised")));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn what_plex_has_no_equivalent_for_and_whatever_names_a_member_is_answered_unsupported() {
    let key = a_key();
    for (operation, asked) in [
        ("signed_in", r#"{"member":"m","device":"d"}"#),
        ("title", r#"{"member":"m","id":"i"}"#),
        ("part_way", r#"{"member":"m","most":1}"#),
        (
            "progressed",
            r#"{"member":"m","id":"i","how_far":{"position":1,"ended":false}}"#,
        ),
        ("sign_out", r#"{"device":"d"}"#),
        ("picture", r#"{"member":"m","id":"i","which":"poster"}"#),
        ("holdings", r#"{"member":"m","most":1}"#),
        ("filed_as", r#"{"app":"a"}"#),
        ("known_proxies", "{}"),
        ("trust_only", r#"{"address":"10.0.0.1"}"#),
        ("allowed_origins", "{}"),
        ("allow_only", r#"{"origin":"https://example.org"}"#),
        ("restart", "{}"),
    ] {
        let fake = Fake::silent();
        let (status, body) = answered_by(
            &fake,
            "POST",
            &format!("/lemonfiber/media.serve/v1/{operation}"),
            Some(&key),
            asked,
        )
        .await;
        assert_eq!(
            kind_of(&body),
            Some(serde_json::json!("unsupported")),
            "{operation}: {status}"
        );
        assert!(fake.requests().is_empty(), "{operation}");
    }
}

#[tokio::test]
async fn a_guard_is_told_what_it_enforces_without_plex_being_asked() {
    let fake = Fake::silent();
    let (status, body) = answered_by(
        &fake,
        "POST",
        "/lemonfiber/media.serve/v1/declared",
        Some(&a_key()),
        "{}",
    )
    .await;
    let declared: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    let asked = |field: &str| {
        declared
            .get("question")
            .and_then(|question| question.get(field))
            .cloned()
    };
    assert_eq!(status, StatusCode::OK);
    assert_eq!(asked("to"), Some(serde_json::json!("adapter")));
    assert_eq!(
        asked("path"),
        Some(serde_json::json!(
            "/lemonfiber/media.serve/v1/may?space={space}&item={item}"
        ))
    );
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn the_guards_question_is_asked_of_plex_with_the_members_token_alone() {
    let fake = Fake::always(Answer::reply(
        200,
        r#"{"MediaContainer":{"size":1,"Metadata":[{"ratingKey":"4"}]}}"#,
    ));
    let (status, _) = answered_by(
        &fake,
        "GET",
        "/lemonfiber/media.serve/v1/may?space=metadata&item=4&X-Plex-Token=the-members-own-token-0123",
        None,
        "",
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let asked: Vec<(String, Vec<String>)> = fake
        .requests()
        .into_iter()
        .map(|one| {
            let tokens = one
                .headers
                .iter()
                .filter(|(name, _)| name == "X-Plex-Token")
                .map(|(_, value)| value.clone())
                .collect();
            (one.url, tokens)
        })
        .collect();
    assert_eq!(
        asked,
        [(
            format!("{ADDRESS}/library/metadata/4"),
            vec!["the-members-own-token-0123".to_owned()]
        )]
    );
}

#[tokio::test]
async fn the_guards_question_carrying_no_token_or_the_plugins_key_is_no_session() {
    for query in ["", &format!("&X-Plex-Token={}", a_key())] {
        let fake = Fake::always(Answer::reply(200, "{}"));
        let (status, _) = answered_by(
            &fake,
            "GET",
            &format!("/lemonfiber/media.serve/v1/may?space=metadata&item=4{query}"),
            None,
            "",
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{query}");
        assert!(fake.requests().is_empty(), "{query}");
    }
}

#[tokio::test]
async fn a_capability_it_does_not_speak_is_refused_as_unknown() {
    let key = a_key();
    let (status, _) = answered_by(
        &Fake::silent(),
        "POST",
        "/lemonfiber/identity.source/v1/household",
        Some(&key),
        "{}",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn it_is_ready_while_plex_says_which_server_it_is() {
    let key = a_key();
    let up = Fake::always(Answer::reply(200, "."));
    let (status, body) = answered_by(&up, "GET", &ready_path(), Some(&key), "").await;
    assert_eq!(
        (status, body.as_slice()),
        (StatusCode::OK, b"true".as_slice())
    );
    assert!(up
        .requests()
        .iter()
        .all(|asked| asked.url == "http://plex:32400/identity"));
    for down in [Fake::silent(), Fake::always(Answer::reply(503, ""))] {
        let (status, body) = answered_by(&down, "GET", &ready_path(), Some(&key), "").await;
        assert_eq!(
            (status, body.as_slice()),
            (StatusCode::OK, b"false".as_slice())
        );
    }
}
