//! Signing a member in by a PIN plex.tv issues to their device.

use super::*;

/// A PIN plex.tv answers, lapsing at `lapses` and holding `token` once it is approved.
fn pin(lapses: &str, token: Option<&str>) -> String {
    serde_json::json!({
        "id": 4242,
        "code": "WXYZ",
        "expiresAt": lapses,
        "authToken": token,
    })
    .to_string()
}

/// A moment long after any test runs.
const LATER: &str = "2999-01-01T00:00:00Z";

/// A moment before any test runs.
const EARLIER: &str = "2000-01-01T00:00:00Z";

/// A stack whose plex.tv issues a PIN lapsing at `lapses`, answers it with `asked`, and
/// says the token is `whose`'s.
fn pinning(lapses: &str, asked: Answer, whose: &str) -> Arc<Fake> {
    household(vec![
        (
            Some(Method::Post),
            "https://plex.tv/api/v2/pins",
            Answer::reply(201, pin(lapses, None)),
        ),
        (Some(Method::Get), "https://plex.tv/api/v2/pins/4242", asked),
        (
            Some(Method::Get),
            "https://plex.tv/api/v2/user",
            Answer::reply(200, whose),
        ),
    ])
}

#[tokio::test]
async fn a_code_is_a_pin_plex_tv_issues_to_the_device_approved_at_plex_tv_link() {
    let shared = claimed("household-pin");
    let fake = pinning(LATER, Answer::reply(200, pin(LATER, None)), r#"{"id":92}"#);

    let code = plex(&fake, &shared)
        .sign_in_by_code("phone-0123456789")
        .await;

    let code = code.ok();
    let id = code.as_ref().map(|one| one.id.clone()).unwrap_or_default();
    assert_eq!(id.len(), 32, "{id}");
    assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()), "{id}");
    assert!(!id.contains("4242") && !id.contains("phone"), "{id}");
    assert_eq!(code.as_ref().map(|one| one.code.as_str()), Some("WXYZ"));
    assert_eq!(
        code.as_ref().map(|one| one.approve_at.as_str()),
        Some("https://plex.tv/link")
    );
    assert_eq!(
        code.map(|one| one.lapses_at),
        Instant::from_service_text(LATER)
    );
    let asked = fake.requests();
    assert!(asked
        .iter()
        .all(|one| one.headers.iter().any(
            |(name, value)| name == "X-Plex-Client-Identifier" && value == "phone-0123456789"
        )));
    assert!(asked
        .iter()
        .all(|one| !one.headers.iter().any(|(name, _)| name == "X-Plex-Token")));
}

#[tokio::test]
async fn no_device_is_issued_no_code() {
    let shared = claimed("household-pin-device");
    let fake = household(Vec::new());
    assert!(plex(&fake, &shared).sign_in_by_code("").await.is_err());
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_pin_proves_the_member_its_token_is_once_approved_and_nothing_before() {
    for (answered, whose, proved) in [
        (pin(LATER, None), r#"{"id":92}"#, None),
        (pin(LATER, Some("member-token")), r#"{"id":92}"#, Some("92")),
        (pin(LATER, Some("member-token")), r#"{"id":93}"#, None),
    ] {
        let shared = claimed("household-pin-proved");
        let fake = pinning(LATER, Answer::reply(200, answered.clone()), whose);
        let plex = plex(&fake, &shared);
        let id = plex
            .sign_in_by_code("phone-0123456789")
            .await
            .map(|code| code.id)
            .unwrap_or_default();

        let signed = plex.signed_by_code(&id).await.ok().flatten();

        assert_eq!(
            signed.as_ref().map(|one| one.id.as_str()),
            proved,
            "{answered} {whose}"
        );
        if proved.is_some() {
            assert_eq!(signed.map(|one| one.token), Some("member-token".to_owned()));
        }
    }
}

#[tokio::test]
async fn an_id_this_adapter_never_issued_proves_nobody_and_asks_plex_tv_nothing() {
    let shared = claimed("household-pin-forged");
    let fake = pinning(
        LATER,
        Answer::reply(200, pin(LATER, Some("member-token"))),
        r#"{"id":92}"#,
    );
    let plex = plex(&fake, &shared);
    for id in [
        "4242",
        "4242.phone-0123456789",
        "00000000000000000000000000000000",
        "",
    ] {
        assert_eq!(plex.signed_by_code(id).await.ok(), Some(None), "{id}");
    }
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_pin_proves_somebody_once_and_its_id_proves_nobody_again() {
    let shared = claimed("household-pin-spent");
    let fake = pinning(
        LATER,
        Answer::reply(200, pin(LATER, Some("member-token"))),
        r#"{"id":92}"#,
    );
    let plex = plex(&fake, &shared);
    let id = plex
        .sign_in_by_code("phone-0123456789")
        .await
        .map(|code| code.id)
        .unwrap_or_default();

    let first = plex.signed_by_code(&id).await.ok().flatten();
    let again = plex.signed_by_code(&id).await;

    assert_eq!(first.map(|one| one.id), Some("92".to_owned()));
    assert_eq!(again.ok(), Some(None));
}

#[tokio::test]
async fn a_lapsed_pin_or_one_plex_tv_no_longer_holds_proves_nobody() {
    let shared = claimed("household-pin-lapsed");
    let fake = pinning(
        EARLIER,
        Answer::reply(200, pin(EARLIER, Some("member-token"))),
        r#"{"id":92}"#,
    );
    let plex_lapsed = plex(&fake, &shared);
    let id = plex_lapsed
        .sign_in_by_code("phone-0123456789")
        .await
        .map(|code| code.id)
        .unwrap_or_default();
    assert_eq!(plex_lapsed.signed_by_code(&id).await.ok(), Some(None));
    assert!(!fake
        .requests()
        .iter()
        .any(|one| one.url.ends_with("/api/v2/pins/4242")));

    let shared = claimed("household-pin-gone");
    let fake = pinning(LATER, Answer::reply(404, ""), r#"{"id":92}"#);
    let plex_gone = plex(&fake, &shared);
    let id = plex_gone
        .sign_in_by_code("phone-0123456789")
        .await
        .map(|code| code.id)
        .unwrap_or_default();
    assert_eq!(plex_gone.signed_by_code(&id).await.ok(), Some(None));
    assert_eq!(plex_gone.signed_by_code(&id).await.ok(), Some(None));
}

#[tokio::test]
async fn a_pin_plex_tv_refuses_answers_or_dates_unreadably_issues_no_code() {
    for (answer, tag) in [
        (Answer::reply(500, ""), "household-pin-refused"),
        (Answer::reply(201, "not a pin"), "household-pin-unreadable"),
        (
            Answer::reply(201, pin("whenever", None)),
            "household-pin-undated",
        ),
    ] {
        let shared = claimed(tag);
        let fake = household(vec![(
            Some(Method::Post),
            "https://plex.tv/api/v2/pins",
            answer,
        )]);

        assert!(
            plex(&fake, &shared)
                .sign_in_by_code("phone-0123456789")
                .await
                .is_err(),
            "{tag}"
        );
    }
}

/// The ids `plex` issues for each of `devices` in turn.
async fn issued(plex: &crate::plex::Plex, devices: &[&str]) -> Vec<String> {
    let mut ids = Vec::new();
    for device in devices {
        ids.push(
            plex.sign_in_by_code(device)
                .await
                .map(|code| code.id)
                .unwrap_or_default(),
        );
    }
    ids
}

/// Which of `ids` still prove the member, each asked once.
async fn proving(plex: &crate::plex::Plex, ids: &[String]) -> Vec<bool> {
    let mut proved = Vec::new();
    for id in ids {
        proved.push(plex.signed_by_code(id).await.ok().flatten().is_some());
    }
    proved
}

#[tokio::test]
async fn a_device_asking_again_lets_go_of_its_own_older_sign_in_and_nobody_elses() {
    let shared = claimed("household-pin-flooded");
    let fake = pinning(
        LATER,
        Answer::reply(200, pin(LATER, Some("member-token"))),
        r#"{"id":92}"#,
    );
    let plex = plex(&fake, &shared);

    let ids = issued(&plex, &["tv-1", "phone-1", "phone-1", "phone-1"]).await;

    assert_eq!(proving(&plex, &ids).await, [true, false, false, true]);
}

#[tokio::test]
async fn a_full_table_refuses_a_new_sign_in_asking_plex_tv_nothing_and_keeps_every_one_waiting() {
    let shared = claimed("household-pin-full");
    let fake = pinning(
        LATER,
        Answer::reply(200, pin(LATER, Some("member-token"))),
        r#"{"id":92}"#,
    );
    let plex = plex(&fake, &shared).signing_in_under(
        crate::plex::SignIns {
            per_device: 2,
            in_all: 3,
        },
        Arc::new(lemonfiber_random::Os),
    );

    let ids = issued(&plex, &["tv-1", "phone-1", "phone-1"]).await;
    let asked = fake.requests().len();
    let refused = plex.sign_in_by_code("tablet-1").await;

    assert!(refused.is_err());
    assert_eq!(fake.requests().len(), asked);
    assert_eq!(proving(&plex, &ids).await, [true, true, true]);
}

#[tokio::test]
async fn a_token_plex_tv_no_longer_takes_proves_nobody() {
    let shared = claimed("household-pin-refused-token");
    let fake = household(vec![
        (
            Some(Method::Post),
            "https://plex.tv/api/v2/pins",
            Answer::reply(201, pin(LATER, None)),
        ),
        (
            Some(Method::Get),
            "https://plex.tv/api/v2/pins/4242",
            Answer::reply(200, pin(LATER, Some("member-token"))),
        ),
        (
            Some(Method::Get),
            "https://plex.tv/api/v2/user",
            Answer::reply(401, ""),
        ),
    ]);
    let plex = plex(&fake, &shared);
    let id = plex
        .sign_in_by_code("phone-0123456789")
        .await
        .map(|code| code.id)
        .unwrap_or_default();

    assert_eq!(plex.signed_by_code(&id).await.ok(), Some(None));
}

#[tokio::test]
async fn no_code_is_issued_where_no_id_can_be_drawn() {
    let shared = claimed("household-pin-undrawn");
    let fake = pinning(LATER, Answer::reply(200, pin(LATER, None)), r#"{"id":92}"#);
    let plex = plex(&fake, &shared).signing_in_under(
        crate::plex::SignIns::default(),
        Arc::new(lemonfiber_fixtures::ports::Chance::exactly(None)),
    );

    assert!(plex.sign_in_by_code("phone-0123456789").await.is_err());
}

#[test]
fn a_table_filled_after_its_room_was_asked_still_refuses_and_lets_go_of_nobody() {
    let signing = super::super::signing::Signing::under(
        crate::plex::SignIns {
            per_device: 1,
            in_all: 1,
        },
        Arc::new(lemonfiber_random::Os),
    );
    let now = Instant::from_seconds(1_700_000_000);
    let later = now.later(600);

    assert_eq!(signing.room("tablet-1", now), Ok(()));
    assert!(signing.kept((1, "tv-1", later), now).is_ok());
    assert!(signing.kept((2, "tablet-1", later), now).is_err());
    assert!(signing.room("tablet-1", now).is_err());
    assert_eq!(signing.room("tv-1", now), Ok(()));
}
