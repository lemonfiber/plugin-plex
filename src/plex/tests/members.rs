use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Answered, Failure};

use super::{asked, plex, Shared};

const MEMBER: &str = "the-members-own-token-0123";

const FOUND: &str = r#"{"MediaContainer":{"size":1,"Metadata":[{"ratingKey":"4","type":"episode","title":"Pilot","Media":[{"id":3,"Part":[{"id":2,"key":"/library/parts/2/1793462557/file.mp4"}]}]}]}}"#;

const ANOTHER: &str = r#"{"MediaContainer":{"size":1,"Metadata":[{"ratingKey":"9","type":"movie","title":"Other","Media":[{"id":8,"Part":[{"id":7,"key":"/library/parts/7/1793462557/file.mp4"}]}]}]}}"#;

const NONE: &str = r#"{"MediaContainer":{"size":0}}"#;

const PART_ASKED: &str =
    "/library/all?type=1%2C4&part=2&X-Plex-Container-Start=0&X-Plex-Container-Size=1";

async fn answered(
    answer: Answer,
    space: &str,
    item: &str,
) -> (
    Result<Answered, Failure>,
    Vec<(Method, String, Option<String>)>,
) {
    let shared = Shared::holding(&format!("members-{space}-{item}"), None);
    let fake = Fake::always(answer);
    let answered = plex(&fake, &shared).may(space, item, MEMBER).await;
    (answered, asked(&fake))
}

fn as_the_member(path: &str) -> Vec<(Method, String, Option<String>)> {
    vec![(Method::Get, path.to_owned(), Some(MEMBER.to_owned()))]
}

#[tokio::test]
async fn a_part_plex_shows_the_member_among_its_films_and_episodes_may_be_had() {
    let (answer, asked) = answered(Answer::reply(200, FOUND), "part", "2").await;
    assert!(matches!(answer, Ok(Answered::May)));
    assert_eq!(asked, as_the_member(PART_ASKED));
}

#[tokio::test]
async fn a_part_plex_shows_the_member_nothing_under_may_not() {
    let (answer, asked) = answered(Answer::reply(200, NONE), "part", "2").await;
    assert!(matches!(answer, Ok(Answered::MayNot)));
    assert_eq!(asked, as_the_member(PART_ASKED));
}

#[tokio::test]
async fn a_part_plex_answers_with_no_entry_holding_it_may_not() {
    for body in [
        ANOTHER,
        r#"{"MediaContainer":{"size":1,"Metadata":[{"ratingKey":"4"}]}}"#,
    ] {
        let (answer, _) = answered(Answer::reply(200, body), "part", "2").await;
        assert!(matches!(answer, Ok(Answered::MayNot)), "{body}");
    }
}

#[tokio::test]
async fn an_item_plex_answers_with_nothing_or_another_item_may_not() {
    for body in [NONE, ANOTHER, "{\"MediaContainer\":{}}"] {
        let (answer, _) = answered(Answer::reply(200, body), "metadata", "4").await;
        assert!(matches!(answer, Ok(Answered::MayNot)), "{body}");
    }
}

#[tokio::test]
async fn an_item_plex_answers_the_member_may_be_had() {
    let (answer, asked) = answered(Answer::reply(200, FOUND), "metadata", "4").await;
    assert!(matches!(answer, Ok(Answered::May)));
    assert_eq!(asked, as_the_member("/library/metadata/4"));
}

#[tokio::test]
async fn an_item_plex_withholds_from_the_member_may_not_be_had() {
    for status in [400, 403, 404] {
        let (answer, _) = answered(Answer::reply(status, ""), "metadata", "4").await;
        assert!(matches!(answer, Ok(Answered::MayNot)), "{status}");
    }
}

#[tokio::test]
async fn a_token_plex_refuses_is_no_session() {
    for space in ["part", "metadata"] {
        let (answer, _) = answered(Answer::reply(401, ""), space, "4").await;
        assert!(matches!(answer, Ok(Answered::NoSession)), "{space}");
    }
}

#[tokio::test]
async fn a_plex_that_faults_or_answers_unreadably_is_no_answer() {
    let (faulted, _) = answered(Answer::reply(503, ""), "metadata", "4").await;
    assert!(faulted.is_err());
    for space in ["part", "metadata"] {
        let (unreadable, _) = answered(Answer::reply(200, "not json"), space, "2").await;
        assert!(unreadable.is_err(), "{space}");
    }
}

#[tokio::test]
async fn a_space_plex_has_no_ids_in_or_an_id_not_plexs_is_not_asked_about() {
    for (space, item) in [
        ("stream", "4"),
        ("Metadata", "4"),
        ("metadata", "4a"),
        ("part", ""),
        ("part", "123456789012345678901"),
    ] {
        let (answer, asked) = answered(Answer::reply(200, FOUND), space, item).await;
        assert!(matches!(answer, Ok(Answered::MayNot)), "{space} {item}");
        assert!(asked.is_empty(), "{space} {item}");
    }
}
