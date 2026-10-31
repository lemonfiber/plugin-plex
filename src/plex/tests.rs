use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Claim, Failure, Owner, Ownership as _};

use super::{an_id, Plex, SUPPORT};

mod declared;
mod encrypting;
mod members;
mod screening;
mod serving;

pub(super) const BASE: &str = "http://plex:32400";

pub(super) const TOKEN: &str = "the-online-token";

const CODE: &str = "claim-0123456789abcdef";

const CLIENT: &str = "4f1c2b3a4f1c2b3a4f1c2b3a4f1c2b3a4f1c2b3a";

/// The configuration directory Plex runs with, as this adapter shares it, removed when
/// dropped.
pub(super) struct Shared {
    root: PathBuf,
}

impl Shared {
    /// A directory whose preferences hold `preferences`, or none.
    pub(super) fn holding(name: &str, preferences: Option<&str>) -> Self {
        let root = std::env::temp_dir().join(format!("plex-adapter-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let support = SUPPORT.iter().fold(root.clone(), |at, step| at.join(step));
        let _ = std::fs::create_dir_all(&support);
        if let Some(preferences) = preferences {
            let _ = std::fs::write(support.join("Preferences.xml"), preferences);
        }
        Self { root }
    }

    /// A directory whose preferences hold the token of the account that claimed Plex.
    pub(super) fn claimed(name: &str) -> Self {
        Self::holding(
            name,
            Some(&format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<Preferences MachineIdentifier="m" PlexOnlineToken="{TOKEN}"/>"#
            )),
        )
    }

    pub(super) fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub(super) fn plex(fake: &Arc<Fake>, shared: &Shared) -> Plex {
    Plex::new(fake.clone(), BASE, shared.path(), Arc::new(CODE.to_owned()))
}

pub(super) fn identity(claimed: bool) -> Answer {
    Answer::reply(
        200,
        format!(r#"{{"MediaContainer":{{"size":0,"claimed":{claimed},"machineIdentifier":"m"}}}}"#),
    )
}

/// Each request, by method and path, with the token it carried.
pub(super) fn asked(fake: &Fake) -> Vec<(Method, String, Option<String>)> {
    fake.requests()
        .into_iter()
        .map(|one| {
            let token = one
                .headers
                .iter()
                .find(|(name, _)| name == "X-Plex-Token")
                .map(|(_, value)| value.clone());
            (
                one.method,
                one.url.trim_start_matches(BASE).to_owned(),
                token,
            )
        })
        .collect()
}

#[tokio::test]
async fn a_plex_that_does_not_answer_or_answers_with_a_fault_is_settling() {
    let shared = Shared::claimed("settling");
    for fake in [Fake::silent(), Fake::always(Answer::reply(503, ""))] {
        let plex = plex(&fake, &shared);
        assert!(matches!(plex.owner().await, Ok(Owner::Settling)));
        assert!(matches!(
            plex.claim().await,
            Ok(Claim::Held(Owner::Settling))
        ));
        assert!(!plex.answers().await);
    }
}

/// Preferences a fresh Plex writes, naming the identifier it presents to plex.tv.
fn fresh() -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Preferences MachineIdentifier="m" ProcessedMachineIdentifier="{CLIENT}" PlexOnlineToken="" TranscoderTempDirectory="/transcode"/>
"#
    )
}

fn exchanged(token: &str) -> Answer {
    Answer::reply(
        200,
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<user><authentication-token>{token}</authentication-token></user>"
        ),
    )
}

fn preferences_of(shared: &Shared) -> String {
    let at = SUPPORT
        .iter()
        .fold(shared.path().to_path_buf(), |at, step| at.join(step))
        .join("Preferences.xml");
    std::fs::read_to_string(at).unwrap_or_default()
}

#[tokio::test]
async fn an_open_plex_is_claimed_by_exchanging_the_code_and_writing_the_token_it_takes_on_restart()
{
    let shared = Shared::holding("claimed", Some(&fresh()));
    let fake = Fake::by_rules(vec![
        (None, "/identity", identity(false)),
        (Some(Method::Post), "/api/claim/exchange", exchanged(TOKEN)),
    ]);
    let plex = plex(&fake, &shared);

    assert!(matches!(plex.owner().await, Ok(Owner::Open)));
    assert!(matches!(plex.claim().await, Ok(Claim::Restarting(_))));

    let written = preferences_of(&shared);
    assert!(
        written.contains(&format!(r#"PlexOnlineToken="{TOKEN}""#)),
        "{written}"
    );
    assert!(written.contains(&format!(r#"ProcessedMachineIdentifier="{CLIENT}""#)));
    assert!(written.contains(r#"TranscoderTempDirectory="/transcode""#));
    assert_eq!(written.matches("PlexOnlineToken").count(), 1);
    let exchange = fake
        .requests()
        .into_iter()
        .find(|asked| asked.url.contains("/api/claim/exchange"));
    assert_eq!(
        exchange.as_ref().map(|asked| asked.url.clone()),
        Some(format!("https://plex.tv/api/claim/exchange?token={CODE}"))
    );
    assert!(exchange.is_some_and(|asked| asked
        .headers
        .contains(&("X-Plex-Client-Identifier".to_owned(), CLIENT.to_owned()))));
}

#[tokio::test]
async fn a_code_plex_tv_refuses_or_answers_no_token_for_leaves_the_preferences_as_they_were() {
    for answer in [
        Answer::reply(404, "<errors><error>Invalid token</error></errors>"),
        Answer::reply(200, "<user></user>"),
        exchanged("a<b"),
    ] {
        let shared = Shared::holding("exchange-refused", Some(&fresh()));
        let fake = Fake::by_rules(vec![
            (None, "/identity", identity(false)),
            (Some(Method::Post), "/api/claim/exchange", answer),
        ]);
        assert!(plex(&fake, &shared).claim().await.is_err());
        assert_eq!(preferences_of(&shared), fresh());
    }
}

#[tokio::test]
async fn no_code_or_preferences_naming_no_identifier_are_refused_before_plex_tv_is_asked() {
    struct Missing;
    impl lemonfiber_adapter_kit::credential::Holding for Missing {
        fn key(&self) -> Result<String, String> {
            Err("upstream.json holds no code".to_owned())
        }
    }
    let shared = Shared::holding("no-code", Some(&fresh()));
    let fake = Fake::by_path(vec![("/identity", identity(false))]);
    let codeless = Plex::new(fake.clone(), BASE, shared.path(), Arc::new(Missing));
    assert!(matches!(
        codeless.claim().await,
        Err(Failure::Refused { detail, .. }) if detail.contains("no claim code")
    ));

    for preferences in [
        None,
        Some(r#"<Preferences MachineIdentifier="m"/>"#),
        Some("<Other/>"),
    ] {
        let shared = Shared::holding("no-client", preferences);
        assert!(matches!(
            plex(&fake, &shared).claim().await,
            Err(Failure::Refused { detail, .. }) if detail.contains("preferences")
        ));
    }
    assert!(fake
        .requests()
        .iter()
        .all(|asked| !asked.url.contains("plex.tv")));
}

#[tokio::test]
async fn a_claimed_plex_that_takes_the_token_it_keeps_is_ours_and_asks_with_it() {
    let shared = Shared::claimed("ours");
    let fake = Fake::by_path(vec![
        ("/identity", identity(true)),
        ("/:/prefs", Answer::reply(200, r#"{"MediaContainer":{}}"#)),
    ]);
    let plex = plex(&fake, &shared);

    assert!(matches!(plex.owner().await, Ok(Owner::Ours)));
    assert!(matches!(plex.claim().await, Ok(Claim::Held(Owner::Ours))));
    assert_eq!(
        asked(&fake).get(..2),
        Some(
            &[
                (Method::Get, "/identity".to_owned(), None),
                (Method::Get, "/:/prefs".to_owned(), Some(TOKEN.to_owned())),
            ][..]
        )
    );
}

#[tokio::test]
async fn a_claimed_plex_refusing_the_token_or_keeping_none_is_theirs() {
    let refusing = Shared::claimed("refusing");
    let fake = Fake::by_path(vec![
        ("/identity", identity(true)),
        ("/:/prefs", Answer::reply(401, "")),
    ]);
    assert!(matches!(
        plex(&fake, &refusing).owner().await,
        Ok(Owner::Theirs)
    ));

    for shared in [
        Shared::holding("tokenless", Some(r#"<Preferences MachineIdentifier="m"/>"#)),
        Shared::holding("never-written", None),
    ] {
        let fake = Fake::by_path(vec![("/identity", identity(true))]);
        assert!(matches!(
            plex(&fake, &shared).owner().await,
            Ok(Owner::Theirs)
        ));
        assert_eq!(asked(&fake).len(), 1);
    }
}

#[tokio::test]
async fn a_fault_reading_whose_plex_it_is_is_a_failure_and_never_an_owner() {
    let shared = Shared::claimed("faults");
    let unreadable = Fake::by_path(vec![("/identity", Answer::reply(200, "<html>"))]);
    assert!(plex(&unreadable, &shared).owner().await.is_err());

    let faulting = Fake::by_path(vec![
        ("/identity", identity(true)),
        ("/:/prefs", Answer::reply(500, "")),
    ]);
    assert!(plex(&faulting, &shared).owner().await.is_err());

    let gone = Fake::by_rules(vec![
        (None, "/identity", identity(true)),
        (None, "/:/prefs", Answer::Silent),
    ]);
    assert!(plex(&gone, &shared).owner().await.is_err());
}

#[tokio::test]
async fn preferences_reached_through_a_link_are_refused_and_never_read() {
    let shared = Shared::holding("linked", None);
    let support = SUPPORT
        .iter()
        .fold(shared.path().to_path_buf(), |at, step| at.join(step));
    let _ = std::fs::remove_dir_all(&support);
    let elsewhere = shared.path().join("elsewhere");
    let _ = std::fs::create_dir_all(&elsewhere);
    let _ = std::fs::write(
        elsewhere.join("Preferences.xml"),
        format!(r#"<Preferences PlexOnlineToken="{TOKEN}"/>"#),
    );
    let _ = symlink(&elsewhere, &support);
    let fake = Fake::by_path(vec![("/identity", identity(true))]);

    assert!(matches!(
        plex(&fake, &shared).owner().await,
        Err(Failure::Refused { detail, .. }) if detail.contains("preferences")
    ));
    assert_eq!(asked(&fake).len(), 1);
}

#[test]
fn an_id_is_the_digits_plex_files_an_item_under() {
    for id in ["1", "42", "12345678901234567890"] {
        assert!(an_id(id), "{id}");
    }
    for id in [
        "",
        "1a",
        "-1",
        "123456789012345678901",
        "../1",
        "1/children",
    ] {
        assert!(!an_id(id), "{id}");
    }
}

#[tokio::test]
async fn preferences_the_adapter_may_not_write_fail_the_claim_naming_them() {
    use std::os::unix::fs::PermissionsExt as _;
    let shared = Shared::holding("unwritable", Some(&fresh()));
    let support = SUPPORT
        .iter()
        .fold(shared.path().to_path_buf(), |at, step| at.join(step));
    let _ = std::fs::set_permissions(&support, std::fs::Permissions::from_mode(0o555));
    let fake = Fake::by_rules(vec![
        (None, "/identity", identity(false)),
        (Some(Method::Post), "/api/claim/exchange", exchanged(TOKEN)),
    ]);

    let claimed = plex(&fake, &shared).claim().await;
    let _ = std::fs::set_permissions(&support, std::fs::Permissions::from_mode(0o755));

    assert!(matches!(
        claimed,
        Err(Failure::Refused { detail, .. }) if detail.contains("could not be written")
    ));
    assert_eq!(preferences_of(&shared), fresh());
}
