use std::sync::Arc;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{AppKeys as _, Dated, Failure};

use super::{plex, Shared, TOKEN};

const CLIENT: &str = "client-of-the-server";

const MINTED: &str = "managed-account-token";

const NONE_TITLED: &str =
    r#"<MediaContainer size="1"><User id="1" title="Owner" admin="1"/></MediaContainer>"#;

const ONE_TITLED: &str = r#"<MediaContainer size="2"><User id="1" title="Owner" admin="1"/><User id="77" title="lemonfiber-request-gate" restricted="1"/></MediaContainer>"#;

const TWO_TITLED: &str = r#"<MediaContainer size="3"><User id="1" title="Owner" admin="1"/><User id="77" title="lemonfiber-request-gate"/><User id="78" title="lemonfiber-request-gate"/><User id="90" title="Ana"/></MediaContainer>"#;

const SWITCHED: &str = r#"<user id="77" title="lemonfiber-request-gate" authenticationToken="managed-account-token"/>"#;

const IDENTITY: &str = r#"{"MediaContainer":{"claimed":true,"machineIdentifier":"the-machine"}}"#;

const SECTIONS: &str = r#"{"MediaContainer":{"size":2,"Directory":[{"key":"1","type":"movie"},{"key":"2","type":"show"}]}}"#;

const APP: &str = "request-gate";

/// Plex's configuration directory once claimed, naming the identifier it presents to plex.tv.
fn presenting(name: &str) -> Shared {
    Shared::holding(
        name,
        Some(&format!(
            r#"<Preferences MachineIdentifier="m" ProcessedMachineIdentifier="{CLIENT}" PlexOnlineToken="{TOKEN}"/>"#
        )),
    )
}

fn who(id: u64) -> Answer {
    Answer::reply(200, format!(r#"{{"id":{id},"title":"whoever"}}"#))
}

/// Each request, by method and full URL, with the token it carried.
fn sent(fake: &Fake) -> Vec<(Method, String, Option<String>)> {
    fake.requests()
        .into_iter()
        .map(|one| {
            let token = one
                .headers
                .iter()
                .find(|(name, _)| name == "X-Plex-Token")
                .map(|(_, value)| value.clone());
            (one.method, one.url, token)
        })
        .collect()
}

fn deleted(fake: &Fake) -> Vec<String> {
    sent(fake)
        .into_iter()
        .filter(|(method, _, _)| *method == Method::Delete)
        .map(|(_, url, _)| url)
        .collect()
}

fn minting(shared: Answer) -> Arc<Fake> {
    Fake::by_route_in_turn(vec![
        (
            Method::Get,
            "https://plex.tv/api/home/users",
            vec![
                Answer::reply(200, NONE_TITLED),
                Answer::reply(200, ONE_TITLED),
            ],
        ),
        (
            Method::Post,
            "https://plex.tv/api/home/users?title=",
            vec![Answer::reply(201, r#"<User id="77"/>"#)],
        ),
        (Method::Post, "/switch", vec![Answer::reply(201, SWITCHED)]),
        (Method::Post, "/shared_servers", vec![shared]),
        (
            Method::Delete,
            "/api/home/users/77",
            vec![Answer::reply(200, "")],
        ),
        (Method::Get, "/identity", vec![Answer::reply(200, IDENTITY)]),
        (
            Method::Get,
            "/library/sections",
            vec![Answer::reply(200, SECTIONS)],
        ),
    ])
}

#[tokio::test]
async fn a_key_is_a_new_home_account_titled_for_the_app_shared_every_library() {
    let shared = presenting("keys-mint");
    let fake = minting(Answer::reply(200, ""));
    let plex = plex(&fake, &shared);

    let minted = plex.mint(APP).await;
    assert_eq!(
        minted.as_deref().ok(),
        Some(MINTED),
        "{minted:?} {:?}",
        sent(&fake)
    );
    let asked = sent(&fake);
    assert!(asked
        .iter()
        .any(|(method, url, token)| *method == Method::Post
            && url == "https://plex.tv/api/home/users?title=lemonfiber-request-gate"
            && token.as_deref() == Some(TOKEN)));
    assert!(asked.iter().any(|(method, url, _)| *method == Method::Post
        && url == "https://plex.tv/api/servers/the-machine/shared_servers"));
    assert!(asked.iter().any(|(method, url, _)| *method == Method::Post
        && url == "https://plex.tv/api/home/users/77/switch"));
    let share = fake
        .requests()
        .into_iter()
        .find(|one| one.url.ends_with("/shared_servers"))
        .and_then(|one| one.body)
        .unwrap_or_default();
    assert!(share.contains(r#""library_section_ids":[1,2]"#), "{share}");
    assert!(share.contains(r#""invited_id":77"#), "{share}");
    assert!(fake
        .requests()
        .iter()
        .filter(|one| one.url.starts_with("https://plex.tv"))
        .all(|one| one
            .headers
            .iter()
            .any(|(name, value)| name == "X-Plex-Client-Identifier" && value == CLIENT)));
    assert!(deleted(&fake).is_empty());
}

#[tokio::test]
async fn an_account_that_cannot_be_shared_the_libraries_is_removed_again() {
    let shared = presenting("keys-mint-unshared");
    let fake = minting(Answer::reply(500, ""));
    let plex = plex(&fake, &shared);

    assert!(plex.mint(APP).await.is_err());
    assert_eq!(deleted(&fake), ["https://plex.tv/api/home/users/77"]);
}

#[tokio::test]
async fn a_mint_that_leaves_other_than_one_new_account_is_refused() {
    let shared = presenting("keys-mint-none");
    let fake = Fake::by_route_in_turn(vec![
        (
            Method::Get,
            "https://plex.tv/api/home/users",
            vec![Answer::reply(200, NONE_TITLED)],
        ),
        (
            Method::Post,
            "https://plex.tv/api/home/users?title=",
            vec![Answer::reply(201, "")],
        ),
    ]);
    let plex = plex(&fake, &shared);

    assert!(matches!(plex.mint(APP).await, Err(Failure::Refused { .. })));
    assert!(!fake.asked_for("/switch"));
}

#[tokio::test]
async fn the_keys_filed_as_an_app_are_the_tokens_of_its_titled_accounts() {
    let shared = presenting("keys-filed");
    let fake = Fake::by_route(vec![
        (
            Method::Get,
            "https://plex.tv/api/home/users",
            Answer::reply(200, TWO_TITLED),
        ),
        (Method::Post, "/switch", Answer::reply(201, SWITCHED)),
    ]);
    let plex = plex(&fake, &shared);

    assert_eq!(
        plex.filed_as(APP).await.ok(),
        Some(vec![MINTED.to_owned(), MINTED.to_owned()])
    );
    let switched: Vec<String> = sent(&fake)
        .into_iter()
        .filter(|(_, url, _)| url.ends_with("/switch"))
        .map(|(_, url, _)| url)
        .collect();
    assert_eq!(
        switched,
        [
            "https://plex.tv/api/home/users/77/switch",
            "https://plex.tv/api/home/users/78/switch"
        ]
    );
    assert_eq!(
        plex.dated(APP).await.ok(),
        Some(vec![
            Dated {
                created: None,
                last_used: None
            };
            2
        ])
    );
}

#[tokio::test]
async fn a_name_that_is_not_one_lowercase_word_is_refused_before_anything_is_asked() {
    let shared = presenting("keys-bad-app");
    let fake = Fake::always(Answer::reply(200, ONE_TITLED));
    let plex = plex(&fake, &shared);

    for app in ["", "Request Gate", "../x", "a&title=Owner"] {
        assert!(
            matches!(plex.mint(app).await, Err(Failure::Refused { .. })),
            "{app}"
        );
        assert!(
            matches!(plex.filed_as(app).await, Err(Failure::Refused { .. })),
            "{app}"
        );
    }
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_key_is_revoked_by_removing_the_home_account_plex_tv_says_it_is() {
    let shared = presenting("keys-revoke");
    let fake = Fake::by_route(vec![
        (Method::Get, "https://plex.tv/api/v2/user", who(77)),
        (
            Method::Get,
            "https://plex.tv/api/home/users",
            Answer::reply(200, TWO_TITLED),
        ),
        (
            Method::Delete,
            "https://plex.tv/api/home/users/",
            Answer::reply(200, ""),
        ),
    ]);
    let plex = plex(&fake, &shared);

    assert!(plex.revoke(MINTED).await.is_ok());
    assert_eq!(deleted(&fake), ["https://plex.tv/api/home/users/77"]);
    assert!(sent(&fake)
        .iter()
        .any(|(_, url, token)| url == "https://plex.tv/api/v2/user"
            && token.as_deref() == Some(MINTED)));
}

#[tokio::test]
async fn a_token_of_an_account_no_app_was_given_is_never_removed() {
    for (id, name) in [
        (1, "keys-revoke-owner"),
        (90, "keys-revoke-member"),
        (5, "keys-revoke-stranger"),
    ] {
        let shared = presenting(name);
        let fake = Fake::by_route(vec![
            (Method::Get, "https://plex.tv/api/v2/user", who(id)),
            (
                Method::Get,
                "https://plex.tv/api/home/users",
                Answer::reply(200, TWO_TITLED),
            ),
            (
                Method::Delete,
                "https://plex.tv/api/home/users/",
                Answer::reply(200, ""),
            ),
        ]);
        let plex = plex(&fake, &shared);

        assert!(
            matches!(
                plex.revoke("some-token").await,
                Err(Failure::Refused { .. })
            ),
            "{id}"
        );
        assert!(deleted(&fake).is_empty(), "{id}");
    }
}

#[tokio::test]
async fn a_token_plex_tv_no_longer_takes_is_already_revoked() {
    let shared = presenting("keys-revoke-gone");
    let fake = Fake::by_route(vec![
        (
            Method::Get,
            "https://plex.tv/api/v2/user",
            Answer::reply(401, ""),
        ),
        (
            Method::Delete,
            "https://plex.tv/api/home/users/",
            Answer::reply(200, ""),
        ),
    ]);
    let plex = plex(&fake, &shared);

    assert!(plex.revoke(MINTED).await.is_ok());
    assert!(deleted(&fake).is_empty());
}

#[tokio::test]
async fn the_server_answering_a_key_is_asked_with_that_key_alone() {
    let shared = presenting("keys-answers");
    for (status, answers) in [(200, true), (401, false)] {
        let fake = Fake::by_route(vec![(
            Method::Get,
            "/library/sections",
            Answer::reply(status, SECTIONS),
        )]);
        let plex = plex(&fake, &shared);

        assert_eq!(plex.answers_to(MINTED).await.is_ok(), answers);
        assert_eq!(
            sent(&fake)
                .first()
                .and_then(|(_, _, token)| token.clone())
                .as_deref(),
            Some(MINTED)
        );
    }
}

#[tokio::test]
async fn a_plex_nobody_has_claimed_revokes_nothing_and_asks_plex_tv_nothing() {
    let shared = Shared::holding("keys-revoke-unclaimed", None);
    let fake = Fake::always(Answer::reply(200, ""));
    let plex = plex(&fake, &shared);

    assert!(matches!(
        plex.revoke(MINTED).await,
        Err(Failure::Unauthorised { .. })
    ));
    assert!(fake.requests().is_empty());
}
