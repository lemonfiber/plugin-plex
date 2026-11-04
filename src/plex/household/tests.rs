use std::sync::Arc;

use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    Access, Allowed, Certificate, Failure, Household as _, Invited, MediaServer as _, Member,
    NamedLibrary, Session, Signed, Unrated,
};

use super::super::tests::{plex, presenting, Shared};
use super::limits::{closing, written as written_limit, CLOSED};

/// Cy's filter, refusing what is certified 15 and over.
const CY: &str = "contentRating!=gb/15%2Cgb/18";

/// Dee's filter, admitting only what is certified U or PG.
const DEE: &str = "contentRating=gb/U%2Cgb/PG";

const IDENTITY: &str = r#"{"MediaContainer":{"claimed":true,"machineIdentifier":"the-machine"}}"#;

/// The owner; the managed account this adapter made for the request service; a managed
/// account titled for an app it never recorded; a managed child; a Home member with a
/// plex.tv account of their own and a share; another without a share; and one id nobody
/// spells.
const HOME: &str = r#"<MediaContainer size="7"><User id="1" title="Owner" admin="1"/><User id="77" title="lemonfiber-request-gate" restricted="1"/><User id="78" title="lemonfiber-other" restricted="1"/><User id="90" title="Kid" restricted="1" restrictionProfile="older_kid"/><User id="91" title="Bo" restrictionProfile=""/><User id="96" title="Ed"/><User id="x" title="Broken"/></MediaContainer>"#;

/// Bo's share of this server; Cy's, narrowed to two libraries and refusing certificates;
/// the child, shared nothing here; somebody shared another server; and the request
/// service's account.
const ACCEPTED: &str = r#"[
{"id":91,"title":"Bo","home":true,"status":"accepted","friendshipCreatedAt":"2026-10-01T00:00:00Z","sharedServers":[{"id":501,"machineIdentifier":"the-machine","allLibraries":true,"libraries":[],"lastSeenAt":"2026-11-01T10:00:00Z"}]},
{"id":92,"title":"Cy","status":"accepted","friendshipCreatedAt":"2026-09-01T00:00:00Z","sharingSettings":{"filterMovies":"contentRating!=gb/15%2Cgb/18"},"sharedServers":[{"id":502,"machineIdentifier":"the-machine","allLibraries":false,"libraries":[{"key":1},{"key":"2"},{"key":null}]}]},
{"id":90,"title":"Kid","restricted":true,"home":true,"status":"accepted","friendshipCreatedAt":"2026-08-01T00:00:00Z","sharedServers":[]},
{"id":93,"title":"Elsewhere","status":"accepted","sharedServers":[{"id":503,"machineIdentifier":"another-machine","allLibraries":true}]},
{"id":77,"title":"lemonfiber-request-gate","restricted":true,"home":true,"status":"accepted","sharedServers":[{"id":505,"machineIdentifier":"the-machine","allLibraries":true}]}
]"#;

/// An invitation still waiting, admitting two certificates and nothing unrated.
const PENDING: &str = r#"[{"id":94,"title":"dee@example.org","status":"pending_sent","friendshipCreatedAt":"2026-11-03T00:00:00Z","sharedServers":[{"id":504,"machineIdentifier":"the-machine","allLibraries":true,"sharingSettings":{"filterMovies":"contentRating=gb/U%2Cgb/PG"}}]}]"#;

const BRITISH: &str = r#"{"id":1,"country":"gb"}"#;

const AT_PLEX_TV: &str = r#"<MediaContainer size="1"><Server machineIdentifier="the-machine"><Section id="1001" key="1"/><Section id="1002" key="2"/></Server></MediaContainer>"#;

const SECTIONS: &str = r#"{"MediaContainer":{"Directory":[{"key":"1","title":"Films"},{"key":"2","title":"Series"},{"key":"x","title":"Odd"}]}}"#;

const FILMS_RATED: &str = r#"{"MediaContainer":{"Directory":[{"title":"gb/12A"},{"title":"gb/15"},{"title":"de/12"},{"title":"PG-13"}]}}"#;

const SERIES_RATED: &str =
    r#"{"MediaContainer":{"Directory":[{"title":"gb/U"},{"title":"gb/12A"},{"title":"gb/12"}]}}"#;

const PLAYING: &str = r#"{"MediaContainer":{"Metadata":[
{"ratingKey":"5","title":"A film","type":"movie","User":{"id":"92","title":"Cy"},"Player":{"machineIdentifier":"tv-1","title":"Living room","product":"Plex for LG","state":"playing"}},
{"ratingKey":"6","title":"Another","type":"movie","User":{"id":"1","title":"Owner"},"Player":{"machineIdentifier":"phone","title":"Phone","product":"Plex for iOS","state":"paused"}},
{"ratingKey":"7","title":"Unplayed","type":"movie","User":{"id":"92","title":"Cy"}}
]}}"#;

const SHARED: &str =
    r#"{"id":506,"invitedId":95,"accepted":false,"invitedEmail":"new@example.org"}"#;

type Rule = (Option<Method>, &'static str, Answer);

/// A claimed Plex whose household is the one above, `first` answering before the rest.
fn household(first: Vec<Rule>) -> Arc<Fake> {
    let mut rules = first;
    rules.extend([
        (Some(Method::Get), "/identity", Answer::reply(200, IDENTITY)),
        (
            Some(Method::Get),
            "status=accepted",
            Answer::reply(200, ACCEPTED),
        ),
        (
            Some(Method::Get),
            "status=pending_sent",
            Answer::reply(200, PENDING),
        ),
        (
            Some(Method::Get),
            "https://plex.tv/api/home/users",
            Answer::reply(200, HOME),
        ),
        (
            Some(Method::Get),
            "https://plex.tv/api/v2/user",
            Answer::reply(200, BRITISH),
        ),
        (
            Some(Method::Get),
            "https://plex.tv/api/servers/the-machine",
            Answer::reply(200, AT_PLEX_TV),
        ),
        (
            Some(Method::Get),
            "/library/sections/1/contentRating",
            Answer::reply(200, FILMS_RATED),
        ),
        (
            Some(Method::Get),
            "/library/sections/2/contentRating",
            Answer::reply(200, SERIES_RATED),
        ),
        (
            Some(Method::Get),
            "/library/sections",
            Answer::reply(200, SECTIONS),
        ),
        (
            Some(Method::Get),
            "/status/sessions",
            Answer::reply(200, PLAYING),
        ),
        (
            Some(Method::Post),
            "https://plex.tv/api/v2/shared_servers",
            Answer::reply(201, SHARED),
        ),
        (None, "", Answer::reply(200, "")),
    ]);
    Fake::by_rules(rules)
}

/// Plex's configuration directory, as an adapter that made account 77 for the request
/// service.
fn claimed(name: &str) -> Shared {
    let shared = presenting(name);
    let _ = std::fs::write(
        shared.adapter().join("plex-app-accounts.json"),
        r#"{"apps":{"lemonfiber-request-gate":["77"]}}"#,
    );
    shared
}

/// Each write asked, by method and full URL, with its body.
fn written(fake: &Fake) -> Vec<(Method, String, String)> {
    fake.requests()
        .into_iter()
        .filter(|one| one.method != Method::Get)
        .map(|one| (one.method, one.url, one.body.unwrap_or_default()))
        .collect()
}

/// Each write asked, by method and full URL, with its body read as JSON.
fn written_json(fake: &Fake) -> Vec<(Method, String, serde_json::Value)> {
    written(fake)
        .into_iter()
        .map(|(method, url, body)| (method, url, serde_json::from_str(&body).unwrap_or_default()))
        .collect()
}

fn refused_naming(outcome: Result<(), Failure>, words: &str) -> bool {
    matches!(outcome, Err(Failure::Refused { detail, .. }) if detail.contains(words))
}

#[tokio::test]
async fn the_household_is_the_owners_home_and_this_servers_shares_never_an_apps_account() {
    let shared = claimed("household-read");
    let fake = household(Vec::new());

    let member =
        |id: &str, name: &str, claimed: bool, access: Access, last_seen: Option<&str>| Member {
            id: id.to_owned(),
            name: name.to_owned(),
            claimed,
            access,
            last_seen: last_seen.map(str::to_owned),
        };
    assert_eq!(
        plex(&fake, &shared).household().await.ok(),
        Some(vec![
            member(
                "90",
                "Kid",
                true,
                Access {
                    age_limit: Some(12),
                    unrated: Unrated::HeldBack,
                    disabled: true,
                    ..Access::default()
                },
                None,
            ),
            member(
                "91",
                "Bo",
                true,
                Access {
                    every_library: true,
                    ..Access::default()
                },
                Some("2026-11-01T10:00:00Z"),
            ),
            member(
                "96",
                "Ed",
                true,
                Access {
                    disabled: true,
                    ..Access::default()
                },
                None,
            ),
            member(
                "92",
                "Cy",
                true,
                Access {
                    libraries: vec!["1".to_owned(), "2".to_owned()],
                    age_limit: Some(14),
                    ..Access::default()
                },
                None,
            ),
            member(
                "94",
                "dee@example.org",
                false,
                Access {
                    every_library: true,
                    age_limit: Some(8),
                    unrated: Unrated::HeldBack,
                    ..Access::default()
                },
                None,
            ),
        ])
    );
    assert!(written(&fake).is_empty());
}

#[tokio::test]
async fn a_household_plex_tv_will_not_list_readably_is_a_failure() {
    for broken in [
        (
            Some(Method::Get),
            "status=accepted",
            Answer::reply(200, "{"),
        ),
        (
            Some(Method::Get),
            "https://plex.tv/api/home/users",
            Answer::reply(500, ""),
        ),
    ] {
        let shared = claimed("household-broken");
        let fake = household(vec![broken]);
        assert!(plex(&fake, &shared).household().await.is_err());
    }
}

#[tokio::test]
async fn no_password_is_checked_set_or_changed_over_plex() {
    let shared = claimed("household-passwords");
    let fake = household(Vec::new());
    let plex = plex(&fake, &shared);

    assert!(matches!(
        plex.whoever("Cy", "secret", "device").await,
        Err(Failure::Unsupported { .. })
    ));
    assert!(matches!(
        plex.claim("Cy", "secret", "device").await,
        Err(Failure::Unsupported { .. })
    ));
    assert!(matches!(
        plex.create_admin("admin", "secret").await,
        Err(Failure::Unsupported { .. })
    ));
    assert!(matches!(
        plex.replace_admin_password("secret").await,
        Err(Failure::Unsupported { .. })
    ));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_sign_in_stands_while_plex_tv_takes_its_token_as_a_member_who_may_watch_here() {
    let signed = |id: &str| Signed {
        id: id.to_owned(),
        token: "member-token".to_owned(),
    };
    for (whose, id, stands) in [
        (r#"{"id":92}"#, "92", true),
        (r#"{"id":92}"#, "91", false),
        (r#"{"id":90}"#, "90", false),
        (r#"{"id":93}"#, "93", false),
    ] {
        let shared = claimed("household-standing");
        let fake = household(vec![(
            Some(Method::Get),
            "https://plex.tv/api/v2/user",
            Answer::reply(200, whose),
        )]);
        let standing = plex(&fake, &shared).standing(&signed(id)).await;
        assert_eq!(standing.ok(), Some(stands), "{whose} {id}");
    }

    let shared = claimed("household-standing-refused");
    let fake = household(vec![(
        Some(Method::Get),
        "https://plex.tv/api/v2/user",
        Answer::reply(401, ""),
    )]);
    assert_eq!(
        plex(&fake, &shared).standing(&signed("92")).await.ok(),
        Some(false)
    );
    let asked: Vec<String> = fake.requests().into_iter().map(|one| one.url).collect();
    assert_eq!(asked, ["https://plex.tv/api/v2/user"]);
}

#[tokio::test]
async fn an_invitation_shares_every_library_closed_until_its_account_is_made_claimable() {
    let shared = claimed("household-invite");
    let fake = household(Vec::new());

    let invited = plex(&fake, &shared).invite("new@example.org").await;

    assert_eq!(
        invited.ok(),
        Some(Member {
            id: "95".to_owned(),
            name: "new@example.org".to_owned(),
            claimed: false,
            access: Access {
                every_library: true,
                disabled: true,
                ..Access::default()
            },
            last_seen: None,
        })
    );
    assert_eq!(
        written_json(&fake),
        [(
            Method::Post,
            "https://plex.tv/api/v2/shared_servers".to_owned(),
            serde_json::json!({
                "machineIdentifier": "the-machine",
                "invitedEmail": "new@example.org",
                "librarySectionIds": [1001, 1002],
                "settings": closed_settings(),
            })
        )]
    );
}

#[tokio::test]
async fn an_invitation_naming_nobody_or_answered_without_an_account_is_refused() {
    let shared = claimed("household-invite-empty");
    let fake = household(Vec::new());
    assert!(matches!(
        plex(&fake, &shared).invite("  ").await,
        Err(Failure::Refused { .. })
    ));
    assert!(fake.requests().is_empty());

    for answer in [
        Answer::reply(201, "<html>"),
        Answer::reply(201, r#"{"id":506,"accepted":false}"#),
    ] {
        let fake = household(vec![(
            Some(Method::Post),
            "https://plex.tv/api/v2/shared_servers",
            answer,
        )]);
        assert!(matches!(
            plex(&fake, &shared).invite("new@example.org").await,
            Err(Failure::Refused { .. })
        ));
    }
}

#[tokio::test]
async fn an_invitation_still_waiting_is_taken_back_by_removing_its_share() {
    let shared = claimed("household-unclaim");
    let fake = household(Vec::new());
    let plex = plex(&fake, &shared);

    assert!(plex.unclaim("94").await.is_ok());
    assert!(refused_naming(
        plex.unclaim("92").await,
        "took the invitation up"
    ));
    assert!(refused_naming(plex.unclaim("93").await, "no member"));
    assert_eq!(
        written(&fake),
        [(
            Method::Delete,
            "https://plex.tv/api/servers/the-machine/shared_servers/504".to_owned(),
            String::new()
        )]
    );
}

#[tokio::test]
async fn a_member_is_withdrawn_by_removing_their_share_or_their_home_account() {
    let shared = claimed("household-withdraw");
    let fake = household(Vec::new());
    let plex = plex(&fake, &shared);

    for id in ["92", "91", "90"] {
        assert!(plex.withdraw(id).await.is_ok(), "{id}");
    }
    assert!(refused_naming(plex.withdraw("77").await, "no member"));
    let urls: Vec<String> = written(&fake).into_iter().map(|(_, url, _)| url).collect();
    assert_eq!(
        urls,
        [
            "https://plex.tv/api/servers/the-machine/shared_servers/502",
            "https://plex.tv/api/home/users/91",
            "https://plex.tv/api/home/users/90",
        ]
    );
}

#[tokio::test]
async fn an_invitation_is_dated_by_when_plex_tv_linked_its_account() {
    let shared = claimed("household-when");
    let fake = household(Vec::new());

    let since = plex(&fake, &shared)
        .when_invited("2026-09-15T00:00:00Z")
        .await;

    assert_eq!(
        since.ok(),
        Some(vec![
            Invited {
                member: "91".to_owned(),
                at: "2026-10-01T00:00:00Z".to_owned(),
            },
            Invited {
                member: "94".to_owned(),
                at: "2026-11-03T00:00:00Z".to_owned(),
            },
        ])
    );
}

#[tokio::test]
async fn the_libraries_are_plexs_by_the_key_it_files_each_under() {
    let shared = claimed("household-libraries");
    let fake = household(Vec::new());

    assert_eq!(
        plex(&fake, &shared).libraries().await.ok(),
        Some(vec![
            NamedLibrary {
                id: "1".to_owned(),
                name: "Films".to_owned(),
            },
            NamedLibrary {
                id: "2".to_owned(),
                name: "Series".to_owned(),
            },
        ])
    );
}

#[tokio::test]
async fn the_ratings_are_the_certificates_of_the_owners_country_the_libraries_hold() {
    let shared = claimed("household-ratings");
    let fake = household(Vec::new());

    let named = |name: &str, age: u32| Certificate {
        name: name.to_owned(),
        age,
    };
    assert_eq!(
        plex(&fake, &shared).ratings().await.ok(),
        Some(vec![
            named("U", 0),
            named("12A", 12),
            named("12", 12),
            named("15", 15),
        ])
    );
}

#[tokio::test]
async fn a_country_the_table_does_not_age_is_answered_no_certificates_without_reading_any() {
    for owner in [r#"{"id":1,"country":"zz"}"#, r#"{"id":1}"#] {
        let shared = claimed("household-ratings-unaged");
        let fake = household(vec![(
            Some(Method::Get),
            "https://plex.tv/api/v2/user",
            Answer::reply(200, owner),
        )]);
        assert_eq!(plex(&fake, &shared).ratings().await.ok(), Some(Vec::new()));
        assert!(!fake.asked_for("contentRating"));
    }
}

#[tokio::test]
async fn the_country_is_the_one_the_owners_plex_tv_account_names() {
    for (owner, country) in [
        (r#"{"id":1,"country":"gb"}"#, Some("GB")),
        (r#"{"id":1,"country":"Netherlands"}"#, None),
        (r#"{"id":1,"country":"n1"}"#, None),
        (r#"{"id":1,"country":null}"#, None),
    ] {
        let shared = claimed("household-country");
        let fake = household(vec![(
            Some(Method::Get),
            "https://plex.tv/api/v2/user",
            Answer::reply(200, owner),
        )]);
        assert_eq!(
            plex(&fake, &shared).country().await.ok(),
            Some(country.map(str::to_owned)),
            "{owner}"
        );
    }

    let shared = claimed("household-country-unreadable");
    let fake = household(vec![(
        Some(Method::Get),
        "https://plex.tv/api/v2/user",
        Answer::reply(200, "["),
    )]);
    assert!(matches!(
        plex(&fake, &shared).country().await,
        Err(Failure::Refused { detail, .. }) if detail.contains("unreadably")
    ));
}

fn libraries(keys: &[&str]) -> Allowed {
    Allowed {
        libraries: Some(keys.iter().map(|key| (*key).to_owned()).collect()),
        ..Allowed::default()
    }
}

#[tokio::test]
async fn libraries_change_only_while_the_share_admits_nothing_and_a_new_share_is_filtered_from_the_start(
) {
    let shared = claimed("household-open");
    let fake = household(Vec::new());
    let plex = plex(&fake, &shared);

    assert!(plex.allow("92", &libraries(&["2"])).await.is_ok());
    assert!(plex.claimable("90", &libraries(&["1", "2"])).await.is_ok());

    assert_eq!(
        written_json(&fake),
        [
            (
                Method::Put,
                filtered("92", &closing(CY)),
                serde_json::Value::Null
            ),
            (
                Method::Put,
                "https://plex.tv/api/servers/the-machine/shared_servers/502".to_owned(),
                serde_json::json!({
                    "server_id": "the-machine",
                    "shared_server": { "library_section_ids": [1002] },
                })
            ),
            (Method::Put, filtered("92", CY), serde_json::Value::Null),
            (
                Method::Post,
                "https://plex.tv/api/servers/the-machine/shared_servers".to_owned(),
                serde_json::json!({
                    "server_id": "the-machine",
                    "shared_server": { "library_section_ids": [1001, 1002], "invited_id": 90 },
                    "sharing_settings": settings(""),
                })
            ),
        ]
    );
}

#[tokio::test]
async fn a_library_plex_tv_does_not_list_is_refused_and_nothing_is_written() {
    let shared = claimed("household-open-unknown");
    let fake = household(Vec::new());

    assert!(refused_naming(
        plex(&fake, &shared).allow("92", &libraries(&["9"])).await,
        "no library"
    ));
    assert!(written(&fake).is_empty());
}

/// The URL every filter of the shares with the account `id` is written at, as `filter`.
fn filtered(id: &str, filter: &str) -> String {
    format!(
        "https://plex.tv/api/v2/sharings/{id}?{}",
        form_encoded(&[
            ("filterMovies", filter),
            ("filterTelevision", filter),
            ("filterMusic", filter),
            ("filterPhotos", filter),
        ])
    )
}

/// A share's settings holding every filter to `filter`.
fn settings(filter: &str) -> serde_json::Value {
    serde_json::json!({
        "filterMovies": filter,
        "filterTelevision": filter,
        "filterMusic": filter,
        "filterPhotos": filter,
    })
}

fn closed_settings() -> serde_json::Value {
    settings(CLOSED)
}

fn written_filter(age_limit: Option<u32>, unrated: Unrated) -> String {
    written_limit(age_limit, unrated).unwrap_or_default()
}

#[tokio::test]
async fn an_age_limit_on_a_share_is_written_as_one_filter_over_every_kind_of_library() {
    let shared = claimed("household-limit");
    let fake = household(Vec::new());
    let plex = plex(&fake, &shared);

    let limit = |age_limit: Option<u32>, unrated: Option<Unrated>| Allowed {
        libraries: None,
        age_limit,
        unrated,
    };
    assert!(plex.allow("92", &limit(Some(12), None)).await.is_ok());
    assert!(plex
        .allow("92", &limit(Some(12), Some(Unrated::HeldBack)))
        .await
        .is_ok());
    assert!(plex
        .allow("92", &limit(None, Some(Unrated::HeldBack)))
        .await
        .is_ok());
    assert!(plex.allow("94", &limit(None, None)).await.is_ok());

    let refusing_over_12 = written_filter(Some(12), Unrated::LetThrough);
    let admitting_to_12 = written_filter(Some(12), Unrated::HeldBack);
    let admitting_to_14 = written_filter(Some(14), Unrated::HeldBack);
    let urls: Vec<String> = written(&fake).into_iter().map(|(_, url, _)| url).collect();
    assert_eq!(
        urls,
        [
            filtered("92", &refusing_over_12),
            filtered("92", &admitting_to_12),
            filtered("92", &admitting_to_14),
            filtered("94", DEE),
        ]
    );
}

#[tokio::test]
async fn an_age_limit_on_a_managed_account_is_the_restriction_profile_nearest_under_it() {
    let shared = claimed("household-profile");
    let fake = household(Vec::new());
    let plex = plex(&fake, &shared);

    let limit = |age_limit: Option<u32>| Allowed {
        libraries: None,
        age_limit,
        unrated: Some(Unrated::HeldBack),
    };
    assert!(plex.allow("90", &limit(Some(15))).await.is_ok());
    assert!(plex.allow("90", &limit(Some(18))).await.is_ok());
    assert!(plex.allow("90", &limit(None)).await.is_ok());

    let urls: Vec<String> = written(&fake).into_iter().map(|(_, url, _)| url).collect();
    assert_eq!(
        urls,
        [
            "https://plex.tv/api/v2/home/users/restricted/90?restrictionProfile=older_kid",
            "https://plex.tv/api/v2/home/users/restricted/90?restrictionProfile=",
        ]
    );
}

#[tokio::test]
async fn an_age_limit_under_every_restriction_profile_is_refused_and_opens_nothing() {
    let shared = claimed("household-profile-under");
    let fake = household(Vec::new());

    let allowed = Allowed {
        libraries: Some(vec!["1".to_owned()]),
        age_limit: Some(3),
        unrated: None,
    };
    assert!(refused_naming(
        plex(&fake, &shared).claimable("90", &allowed).await,
        "youngest restriction profile"
    ));
    assert!(written(&fake).is_empty());
}

#[tokio::test]
async fn a_restriction_profile_plex_tv_refuses_leaves_every_library_shut() {
    let shared = claimed("household-profile-refused");
    let fake = household(vec![(
        Some(Method::Put),
        "/api/v2/home/users/restricted/",
        Answer::reply(500, ""),
    )]);

    let allowed = Allowed {
        libraries: Some(vec!["1".to_owned(), "2".to_owned()]),
        age_limit: Some(12),
        unrated: None,
    };
    assert!(plex(&fake, &shared)
        .claimable("90", &allowed)
        .await
        .is_err());
    let urls: Vec<String> = written(&fake).into_iter().map(|(_, url, _)| url).collect();
    assert_eq!(
        urls,
        ["https://plex.tv/api/v2/home/users/restricted/90?restrictionProfile=older_kid"]
    );
}

#[tokio::test]
async fn a_library_write_plex_tv_refuses_leaves_the_share_admitting_nothing() {
    let shared = claimed("household-libraries-refused");
    let fake = household(vec![(
        Some(Method::Put),
        "/shared_servers/502",
        Answer::reply(500, ""),
    )]);

    assert!(plex(&fake, &shared)
        .allow("92", &libraries(&["1", "2"]))
        .await
        .is_err());
    let urls: Vec<String> = written(&fake).into_iter().map(|(_, url, _)| url).collect();
    assert_eq!(
        urls,
        [
            filtered("92", &closing(CY)),
            "https://plex.tv/api/servers/the-machine/shared_servers/502".to_owned(),
        ]
    );
}

#[tokio::test]
async fn a_share_plex_tv_will_not_close_keeps_its_libraries_as_they_were() {
    let shared = claimed("household-close-refused");
    let fake = household(vec![(
        Some(Method::Put),
        "https://plex.tv/api/v2/sharings/",
        Answer::reply(500, ""),
    )]);

    assert!(plex(&fake, &shared)
        .allow("92", &libraries(&["1", "2"]))
        .await
        .is_err());
    assert!(!fake.asked_for("/shared_servers/502"));
}

#[tokio::test]
async fn opening_no_library_closes_a_share_and_never_asks_plex_tv_for_an_empty_list() {
    let shared = claimed("household-open-none");
    let fake = household(Vec::new());
    let plex = plex(&fake, &shared);

    assert!(plex.claimable("92", &libraries(&[])).await.is_ok());
    assert!(plex.claimable("90", &libraries(&[])).await.is_ok());

    let urls: Vec<String> = written(&fake).into_iter().map(|(_, url, _)| url).collect();
    assert_eq!(urls, [filtered("92", &closing(CY))]);
}

#[tokio::test]
async fn an_age_limit_for_a_member_holding_no_share_is_refused_before_anything_is_written() {
    let shared = claimed("household-limit-refused");
    let fake = household(Vec::new());

    let limited = Allowed {
        libraries: Some(vec!["1".to_owned()]),
        age_limit: Some(12),
        unrated: None,
    };
    assert!(refused_naming(
        plex(&fake, &shared).allow("96", &limited).await,
        "no share"
    ));
    assert!(written(&fake).is_empty());
}

#[tokio::test]
async fn suspending_a_member_keeps_their_share_and_limit_and_admits_nothing_until_allowed_again() {
    let shared = claimed("household-suspend");
    let fake = household(Vec::new());
    let plex = plex(&fake, &shared);

    assert!(plex.suspend("92").await.is_ok());
    assert!(plex.suspend("90").await.is_ok());

    let urls: Vec<String> = written(&fake).into_iter().map(|(_, url, _)| url).collect();
    assert_eq!(urls, [filtered("92", &closing(CY))]);
}

#[tokio::test]
async fn a_suspended_member_reads_switched_off_and_signs_in_to_nothing_until_allowed_again() {
    let suspended = format!(
        r#"[{{"id":92,"title":"Cy","status":"accepted","sharedServers":[{{"id":502,"machineIdentifier":"the-machine","allLibraries":true,"sharingSettings":{{"filterMovies":"{}"}}}}]}}]"#,
        closing(CY)
    );
    let shared = claimed("household-suspended");
    let fake = household(vec![
        (
            Some(Method::Get),
            "status=accepted",
            Answer::reply(200, suspended),
        ),
        (
            Some(Method::Get),
            "https://plex.tv/api/v2/user",
            Answer::reply(200, r#"{"id":92}"#),
        ),
    ]);
    let plex = plex(&fake, &shared);

    let cy = plex
        .household()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|member| member.id == "92");
    assert_eq!(
        cy.map(|member| member.access),
        Some(Access {
            every_library: true,
            age_limit: Some(14),
            disabled: true,
            ..Access::default()
        })
    );
    let signed = Signed {
        id: "92".to_owned(),
        token: "member-token".to_owned(),
    };
    assert_eq!(plex.standing(&signed).await.ok(), Some(false));

    assert!(plex.allow("92", &Allowed::default()).await.is_ok());
    let urls: Vec<String> = written(&fake).into_iter().map(|(_, url, _)| url).collect();
    assert_eq!(urls, [filtered("92", CY)]);
}

#[tokio::test]
async fn a_members_sessions_are_the_devices_plex_lists_playing_for_them() {
    let shared = claimed("household-sessions");
    let fake = household(Vec::new());
    let plex = plex(&fake, &shared);

    assert_eq!(
        plex.sessions("92").await.ok(),
        Some(vec![Session {
            device_id: "tv-1".to_owned(),
            device: "Living room".to_owned(),
            client: "Plex for LG".to_owned(),
            last_seen: None,
        }])
    );
    assert_eq!(plex.sessions("../1").await.ok(), Some(Vec::new()));
    assert_eq!(fake.requests().len(), 1);
}

#[tokio::test]
async fn devices_sign_in_by_code_and_setup_is_done_once_plex_is_claimed() {
    let fake = household(Vec::new());
    let shared = claimed("household-setup");
    let plex_claimed = plex(&fake, &shared);
    assert_eq!(plex_claimed.signs_devices_in().await.ok(), Some(true));
    assert_eq!(plex_claimed.startup_completed().await.ok(), Some(true));

    let open = Shared::holding("household-open-plex", None);
    assert_eq!(
        plex(&fake, &open).startup_completed().await.ok(),
        Some(false)
    );
    assert!(fake.requests().is_empty());
}
