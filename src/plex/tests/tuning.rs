use std::sync::Arc;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::instant::Instant;
use lemonfiber_ports::service::{Failure, Tuning as _};

use super::{asked, plex, presenting, TOKEN};

const IDENTITY: &str = r#"{"MediaContainer":{"claimed":true,"machineIdentifier":"the-machine"}}"#;

const HOME: &str =
    r#"<MediaContainer size="1"><User id="1" title="Owner" admin="1"/></MediaContainer>"#;

/// Cy may use the tuners, held to two television certificates; Bo may not; Di may, held
/// to nothing.
const ACCEPTED: &str = r#"[
{"id":92,"title":"Cy","status":"accepted","sharedServers":[{"id":502,"machineIdentifier":"the-machine","allLibraries":true,"sharingSettings":{"allowTuners":1,"filterTelevision":"contentRating=TV-Y%2CTV-G"}}]},
{"id":91,"title":"Bo","status":"accepted","sharedServers":[{"id":501,"machineIdentifier":"the-machine","allLibraries":true,"sharingSettings":{"allowTuners":0}}]},
{"id":95,"title":"Di","status":"accepted","sharedServers":[{"id":505,"machineIdentifier":"the-machine","allLibraries":true,"sharingSettings":{"allowTuners":"2"}}]}
]"#;

const SHARED_SERVERS: &str = r#"<MediaContainer><SharedServer id="502" userID="92" accessToken="cy-own-token-0123456789"/><SharedServer id="505" userID="95" accessToken="di-own-token-0123456789"/></MediaContainer>"#;

const DVRS: &str = r#"{"MediaContainer":{"Dvr":[{"key":"6","lineup":"lineup://tv.plex.providers.epg.cloud/abc#x"},{"key":"7","lineup":"elsewhere"}]}}"#;

const LINEUP: &str =
    r#"{"MediaContainer":{"Channel":[{"gridKey":"g1","title":"One","vcn":"5.1"}]}}"#;

const STREAM: &str = "dc30f95e-6379-44f7-8168-172ffc820496";

const CHANNEL: &str = "6-g1";

fn now() -> u64 {
    Instant::of(std::time::SystemTime::now()).seconds()
}

/// A day of channel g1's guide: a programme rated `first` on now, then one rated `then`.
fn rated(first: &str, then: &str) -> String {
    let at = now();
    format!(
        r#"{{"MediaContainer":{{"Metadata":[
{{"title":"Episode 3","grandparentTitle":"Cartoons","contentRating":"{first}","Media":[{{"beginsAt":{},"endsAt":{}}}]}},
{{"title":"Late film","contentRating":"{then}","Media":[{{"beginsAt":{},"endsAt":{}}}]}}
]}}}}"#,
        at - 600,
        at + 1200,
        at + 1200,
        at + 3000
    )
}

/// A cartoon rated TV-G on now, then a film rated TV-MA.
fn grid() -> String {
    rated("TV-G", "TV-MA")
}

/// A film rated TV-MA on now, then a cartoon rated TV-G.
fn refused_now() -> String {
    rated("TV-MA", "TV-G")
}

fn nothing_airing() -> String {
    r#"{"MediaContainer":{"Metadata":[]}}"#.to_owned()
}

fn tuned() -> String {
    format!(
        r#"{{"MediaContainer":{{"Metadata":[{{"live":true,"Media":[{{"uuid":"{STREAM}"}}]}}]}}}}"#
    )
}

/// A claimed Plex with one DVR whose guide is `guide`, its household the one above.
fn live(guide: String) -> Arc<Fake> {
    live_with(guide, Vec::new())
}

/// The same Plex, answering each of `first` before anything else.
fn live_with(guide: String, first: Vec<(Option<Method>, &'static str, Answer)>) -> Arc<Fake> {
    Fake::by_rules(
        first
            .into_iter()
            .chain([
                (Some(Method::Get), "/identity", Answer::reply(200, IDENTITY)),
                (
                    Some(Method::Get),
                    "status=accepted",
                    Answer::reply(200, ACCEPTED),
                ),
                (
                    Some(Method::Get),
                    "status=pending_sent",
                    Answer::reply(200, "[]"),
                ),
                (
                    Some(Method::Get),
                    "https://plex.tv/api/home/users",
                    Answer::reply(200, HOME),
                ),
                (
                    Some(Method::Get),
                    "https://plex.tv/api/servers/the-machine/shared_servers",
                    Answer::reply(200, SHARED_SERVERS),
                ),
                (
                    Some(Method::Post),
                    "/livetv/dvrs/6/channels/5.1/tune",
                    Answer::reply(200, tuned()),
                ),
                (Some(Method::Get), "/livetv/dvrs", Answer::reply(200, DVRS)),
                (
                    Some(Method::Get),
                    "/tv.plex.providers.epg.cloud:6/lineups/dvr/channels",
                    Answer::reply(200, LINEUP),
                ),
                (
                    Some(Method::Get),
                    "/tv.plex.providers.epg.cloud:6/grid",
                    Answer::reply(200, guide),
                ),
                (
                    Some(Method::Post),
                    "/status/sessions/terminate",
                    Answer::reply(200, ""),
                ),
            ])
            .collect(),
    )
}

fn tuned_asked(fake: &Fake) -> Vec<Option<String>> {
    asked(fake)
        .into_iter()
        .filter(|(method, path, _)| *method == Method::Post && path.ends_with("/tune"))
        .map(|(_, _, token)| token)
        .collect()
}

#[tokio::test]
async fn a_guide_leaves_out_what_the_members_filter_refuses_and_ends_where_plexs_does() {
    let shared = presenting("live-guide");
    let fake = live(grid());

    let guide = plex(&fake, &shared)
        .guide("92", 12)
        .await
        .unwrap_or_else(|why| unreachable!("the guide is answered: {why:?}"));

    let [listing] = guide.as_slice() else {
        unreachable!("one channel is answered: {guide:?}");
    };
    assert_eq!(listing.channel, CHANNEL);
    let titles: Vec<&str> = listing
        .programmes
        .iter()
        .map(|aired| aired.title.as_str())
        .collect();
    assert_eq!(titles, ["Cartoons"]);
    let Some(first) = listing.programmes.first() else {
        unreachable!("a programme is kept");
    };
    assert_eq!(first.certificate.as_deref(), Some("TV-G"));
    let ends = first.ends.seconds();
    assert_eq!(listing.until.map(Instant::seconds), Some(ends + 1800));
    let tokens: Vec<Option<String>> = asked(&fake)
        .into_iter()
        .filter(|(_, path, _)| path.contains("/grid?"))
        .map(|(_, _, token)| token)
        .collect();
    assert!(tokens.contains(&Some(TOKEN.to_owned())));
    assert!(tokens.contains(&Some("cy-own-token-0123456789".to_owned())));
}

#[tokio::test]
async fn a_channel_is_answered_with_the_programme_on_it_now_the_member_may_watch() {
    let shared = presenting("live-channels");
    let fake = live(grid());

    let channels = plex(&fake, &shared)
        .channels("92")
        .await
        .unwrap_or_else(|why| unreachable!("the channels are answered: {why:?}"));

    let [channel] = channels.as_slice() else {
        unreachable!("one channel is answered: {channels:?}");
    };
    assert_eq!(channel.id, CHANNEL);
    assert_eq!(channel.name, "One");
    assert_eq!(channel.number.as_deref(), Some("5.1"));
    let now = channel
        .now
        .as_ref()
        .map(|programme| programme.title.as_str());
    assert_eq!(now, Some("Cartoons"));
}

#[tokio::test]
async fn a_restricted_member_is_answered_no_channel_and_tuned_to_nothing_whose_programme_now_is_refused_or_unknown(
) {
    for (name, guide) in [
        ("live-restricted-refused", refused_now()),
        ("live-restricted-unknown", nothing_airing()),
    ] {
        let shared = presenting(name);
        let fake = live(guide);
        let plex = plex(&fake, &shared);

        assert!(
            matches!(plex.channels("92").await, Ok(channels) if channels.is_empty()),
            "{name}"
        );
        assert!(
            matches!(plex.guide("92", 12).await, Ok(guide) if guide.is_empty()),
            "{name}"
        );
        assert!(matches!(plex.tune("92", CHANNEL).await, Ok(None)), "{name}");
        assert!(tuned_asked(&fake).is_empty(), "{name}");
    }
}

#[tokio::test]
async fn a_member_held_to_nothing_is_answered_a_channel_with_nothing_known_on_it() {
    let shared = presenting("live-unrestricted-unknown");
    let fake = live(nothing_airing());

    let channels = plex(&fake, &shared)
        .channels("95")
        .await
        .unwrap_or_else(|why| unreachable!("the channels are answered: {why:?}"));

    let [channel] = channels.as_slice() else {
        unreachable!("one channel is answered: {channels:?}");
    };
    assert_eq!(channel.now, None);
}

#[tokio::test]
async fn a_member_held_to_nothing_is_tuned_with_their_own_token_to_the_live_session() {
    let shared = presenting("live-tune");
    let fake = live(nothing_airing());

    let tuned = plex(&fake, &shared)
        .tune("95", CHANNEL)
        .await
        .unwrap_or_else(|why| unreachable!("the channel is tuned: {why:?}"))
        .unwrap_or_else(|| unreachable!("Di may watch the channel"));

    assert_eq!(tuned.closes_by, STREAM);
    assert_eq!(
        tuned.direct,
        format!("/livetv/sessions/{STREAM}/lemonfiber/index.m3u8")
    );
    assert_eq!(
        tuned.transcoded,
        format!(
            "/video/:/transcode/universal/start.m3u8?session={STREAM}&path=%2Flivetv%2Fsessions%2F{STREAM}%2Flemonfiber%2Findex.m3u8&protocol=hls&directStream=1"
        )
    );
    assert_eq!(
        tuned_asked(&fake),
        [Some("di-own-token-0123456789".to_owned())]
    );
}

#[tokio::test]
async fn a_member_whose_share_does_not_open_the_tuners_sees_no_channel_and_tunes_nothing() {
    for member in ["91", "404"] {
        let shared = presenting(&format!("live-closed-{member}"));
        let fake = live(grid());
        let plex = plex(&fake, &shared);

        assert!(matches!(plex.channels(member).await, Ok(channels) if channels.is_empty()));
        assert!(matches!(plex.guide(member, 12).await, Ok(guide) if guide.is_empty()));
        assert!(matches!(plex.tune(member, CHANNEL).await, Ok(None)));
        assert!(tuned_asked(&fake).is_empty());
    }
}

#[tokio::test]
async fn plex_without_a_dvr_answers_live_tv_unsupported() {
    let shared = presenting("live-none");
    let fake = Fake::by_rules(vec![(
        Some(Method::Get),
        "/livetv/dvrs",
        Answer::reply(200, r#"{"MediaContainer":{"size":0}}"#),
    )]);
    let plex = plex(&fake, &shared);
    let unsupported =
        |answer: Result<(), Failure>| matches!(answer, Err(Failure::Unsupported { .. }));

    assert!(unsupported(plex.channels("92").await.map(|_| ())));
    assert!(unsupported(plex.guide("92", 12).await.map(|_| ())));
    assert!(unsupported(plex.tune("92", CHANNEL).await.map(|_| ())));
}

#[tokio::test]
async fn a_guide_is_asked_across_one_hour_to_a_week_alone() {
    let shared = presenting("live-hours");
    let fake = Fake::silent();
    let plex = plex(&fake, &shared);

    assert!(plex.guide("92", 0).await.is_err());
    assert!(plex.guide("92", 169).await.is_err());
    assert!(fake.requests().is_empty());
}

fn terminated(fake: &Fake) -> Vec<Option<String>> {
    asked(fake)
        .into_iter()
        .filter(|(method, path, _)| {
            *method == Method::Post
                && path.starts_with("/status/sessions/terminate?sessionId=")
                && path.contains(STREAM)
        })
        .map(|(_, _, token)| token)
        .collect()
}

async fn tuned_for_di(fake: &Arc<Fake>, shared: &super::Shared) {
    assert!(matches!(
        plex(fake, shared).tune("95", CHANNEL).await,
        Ok(Some(tuned)) if tuned.closes_by == STREAM
    ));
}

#[tokio::test]
async fn a_live_session_this_adapter_tuned_is_terminated_by_its_id_once() {
    let shared = presenting("live-untune");
    let fake = live(nothing_airing());
    tuned_for_di(&fake, &shared).await;

    assert!(plex(&fake, &shared).untune(STREAM).await.is_ok());
    assert_eq!(terminated(&fake), [Some(TOKEN.to_owned())]);
    assert!(plex(&fake, &shared).untune(STREAM).await.is_err());
    assert_eq!(terminated(&fake).len(), 1);
}

#[tokio::test]
async fn a_live_session_plex_no_longer_holds_is_closed_already() {
    let shared = presenting("live-untune-gone");
    tuned_for_di(&live(nothing_airing()), &shared).await;
    let gone = Fake::always(Answer::reply(404, ""));

    assert!(plex(&gone, &shared).untune(STREAM).await.is_ok());
    assert!(plex(&Fake::silent(), &shared).untune(STREAM).await.is_err());
}

#[tokio::test]
async fn a_live_session_this_adapter_did_not_tune_is_never_terminated() {
    let shared = presenting("live-untune-other");
    let fake = live(nothing_airing());
    for id in [STREAM, "../other", ""] {
        assert!(plex(&fake, &shared).untune(id).await.is_err(), "{id}");
    }
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_tuned_session_this_adapter_cannot_keep_a_record_of_is_ended_and_refused() {
    use std::os::unix::fs::PermissionsExt as _;
    let shared = presenting("live-unkept");
    let fake = live(nothing_airing());
    let plex = plex(&fake, &shared);
    let read_only = std::fs::Permissions::from_mode(0o500);
    std::fs::set_permissions(shared.adapter(), read_only)
        .unwrap_or_else(|why| unreachable!("the directory is ours: {why}"));

    let tuned = plex.tune("95", CHANNEL).await;
    std::fs::set_permissions(shared.adapter(), std::fs::Permissions::from_mode(0o700))
        .unwrap_or_else(|why| unreachable!("the directory is ours: {why}"));

    assert!(tuned.is_err());
    assert_eq!(terminated(&fake), [Some(TOKEN.to_owned())]);
}

#[tokio::test]
async fn a_plex_keeping_no_record_tunes_and_closes_nothing() {
    let shared = presenting("live-unrecorded");
    let fake = live(nothing_airing());
    let plex = super::Plex::new(
        fake.clone(),
        super::BASE,
        shared.path(),
        Arc::new("claim-0123456789abcdef".to_owned()),
    );

    assert!(plex.tune("95", CHANNEL).await.is_err());
    assert!(plex.untune(STREAM).await.is_err());
    assert!(fake.requests().is_empty());
}

/// Ed and Gu may use the tuners and Fi has no share; neither Ed nor Gu has a token of their
/// share's own, and Gu, a managed account of the owner's Home held to the older kid's
/// profile, is switched to.
fn sharing_without_tokens(guide: String) -> Arc<Fake> {
    live_with(
        guide,
        vec![
            (
                Some(Method::Post),
                "https://plex.tv/api/home/users/98/switch",
                Answer::reply(
                    200,
                    r#"<user authenticationToken="gu-own-token-0123456789"/>"#,
                ),
            ),
            (
                Some(Method::Get),
                "https://plex.tv/api/home/users",
                Answer::reply(
                    200,
                    r#"<MediaContainer size="3"><User id="1" title="Owner" admin="1"/><User id="97" title="Fi"/><User id="98" title="Gu" restricted="1" restrictionProfile="older_kid"/></MediaContainer>"#,
                ),
            ),
            (
                Some(Method::Get),
                "status=accepted",
                Answer::reply(
                    200,
                    r#"[
{"id":96,"title":"Ed","status":"accepted","sharedServers":[{"id":506,"machineIdentifier":"the-machine","allLibraries":true,"sharingSettings":{"allowTuners":1}}]},
{"id":98,"title":"Gu","status":"accepted","home":true,"sharedServers":[{"id":508,"machineIdentifier":"the-machine","allLibraries":true,"sharingSettings":{"allowTuners":1}}]}
]"#,
                ),
            ),
        ],
    )
}

#[tokio::test]
async fn a_member_with_no_share_or_no_token_of_their_own_sees_no_channel_and_a_home_account_is_switched_to(
) {
    let shared = presenting("live-switched");
    let fake = sharing_without_tokens(grid());
    let plex = plex(&fake, &shared);

    assert!(matches!(plex.channels("97").await, Ok(channels) if channels.is_empty()));
    assert!(matches!(plex.channels("96").await, Ok(channels) if channels.is_empty()));
    assert!(matches!(
        plex.tune("98", CHANNEL).await,
        Ok(Some(tuned)) if tuned.closes_by == STREAM
    ));
    assert_eq!(
        tuned_asked(&fake),
        [Some("gu-own-token-0123456789".to_owned())]
    );
}

#[tokio::test]
async fn a_dvr_keyed_by_a_number_is_read_and_one_keyed_by_anything_else_is_passed_over() {
    let shared = presenting("live-keys");
    let fake = live_with(
        nothing_airing(),
        vec![(
            Some(Method::Get),
            "/livetv/dvrs",
            Answer::reply(
                200,
                r#"{"MediaContainer":{"Dvr":[{"key":6,"lineup":"lineup://tv.plex.providers.epg.cloud/abc#x"},{"key":true,"lineup":"lineup://tv.plex.providers.epg.cloud/abc#x"}]}}"#,
            ),
        )],
    );

    let channels = plex(&fake, &shared)
        .channels("95")
        .await
        .unwrap_or_else(|why| unreachable!("the channels are read: {why:?}"));
    assert_eq!(
        channels
            .iter()
            .map(|channel| channel.id.as_str())
            .collect::<Vec<_>>(),
        [CHANNEL]
    );
}

#[tokio::test]
async fn a_programme_without_its_times_is_left_out_of_the_guide_and_one_with_labels_kept() {
    let shared = presenting("live-untimed");
    let at = now();
    let fake = live(format!(
        r#"{{"MediaContainer":{{"Metadata":[
{{"title":"Untimed","contentRating":"TV-G","Media":[{{"beginsAt":1}}]}},
{{"title":"Labelled","contentRating":"TV-G","Label":[{{"tag":"Kids"}}],"Media":[{{"beginsAt":{},"endsAt":{}}}]}}
]}}}}"#,
        at - 60,
        at + 600
    ));

    let guide = plex(&fake, &shared)
        .guide("95", 12)
        .await
        .unwrap_or_else(|why| unreachable!("the guide is read: {why:?}"));
    let titles: Vec<&str> = guide
        .iter()
        .flat_map(|listing| &listing.programmes)
        .map(|programme| programme.title.as_str())
        .collect();
    assert_eq!(titles, ["Labelled"]);
}

#[tokio::test]
async fn a_tune_plex_answers_with_no_live_session_is_refused() {
    let shared = presenting("live-sessionless");
    let fake = live_with(
        nothing_airing(),
        vec![(
            Some(Method::Post),
            "/livetv/dvrs/6/channels/5.1/tune",
            Answer::reply(
                200,
                r#"{"MediaContainer":{"Metadata":[{"live":true,"Media":[{}]}]}}"#,
            ),
        )],
    );

    assert!(plex(&fake, &shared).tune("95", CHANNEL).await.is_err());
    assert!(terminated(&fake).is_empty());
}

#[tokio::test]
async fn a_record_that_cannot_be_read_or_written_ends_no_session_it_cannot_account_for() {
    use std::os::unix::fs::PermissionsExt as _;
    let unreadable = presenting("live-unreadable");
    std::fs::write(
        unreadable.adapter().join("plex-live-sessions.json"),
        "not json",
    )
    .unwrap_or_else(|why| unreachable!("the directory is ours: {why}"));
    let fake = live(nothing_airing());
    assert!(plex(&fake, &unreadable).untune(STREAM).await.is_err());
    assert!(terminated(&fake).is_empty());

    let unwritable = presenting("live-unwritable");
    let fake = live(nothing_airing());
    tuned_for_di(&fake, &unwritable).await;
    std::fs::set_permissions(unwritable.adapter(), std::fs::Permissions::from_mode(0o500))
        .unwrap_or_else(|why| unreachable!("the directory is ours: {why}"));
    let closed = plex(&fake, &unwritable).untune(STREAM).await;
    std::fs::set_permissions(unwritable.adapter(), std::fs::Permissions::from_mode(0o700))
        .unwrap_or_else(|why| unreachable!("the directory is ours: {why}"));

    assert!(closed.is_err());
    assert_eq!(terminated(&fake), [Some(TOKEN.to_owned())]);
}

#[tokio::test]
async fn a_member_the_household_counts_disabled_sees_no_channel_and_tunes_nothing() {
    let shared = presenting("live-disabled");
    let fake = live_with(
        grid(),
        vec![
            (
                Some(Method::Get),
                "status=accepted",
                Answer::reply(
                    200,
                    r#"[
{"id":99,"title":"Hal","status":"accepted","sharedServers":[{"id":509,"machineIdentifier":"the-machine","allLibraries":false,"sharingSettings":{"allowTuners":1}}]},
{"id":100,"title":"Ivy","status":"accepted","sharedServers":[{"id":510,"machineIdentifier":"the-machine","allLibraries":true,"sharingSettings":{"allowTuners":1,"filterMovies":"label=lemonfiber-held"}}]}
]"#,
                ),
            ),
            (
                Some(Method::Get),
                "https://plex.tv/api/servers/the-machine/shared_servers",
                Answer::reply(
                    200,
                    r#"<MediaContainer><SharedServer id="509" userID="99" accessToken="hal-own-token-0123456789"/><SharedServer id="510" userID="100" accessToken="ivy-own-token-0123456789"/></MediaContainer>"#,
                ),
            ),
        ],
    );
    let plex = plex(&fake, &shared);

    for member in ["99", "100"] {
        assert!(
            matches!(plex.channels(member).await, Ok(channels) if channels.is_empty()),
            "{member}"
        );
        assert!(
            matches!(plex.tune(member, CHANNEL).await, Ok(None)),
            "{member}"
        );
    }
    assert!(tuned_asked(&fake).is_empty());
}
