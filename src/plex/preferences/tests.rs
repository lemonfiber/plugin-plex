use super::online_token;

#[test]
fn the_token_is_the_preferences_elements_own_attribute_unescaped() {
    for (preferences, token) in [
        (
            r#"<?xml version="1.0" encoding="utf-8"?>
<Preferences MachineIdentifier="m" PlexOnlineToken="abc-123"/>"#,
            Some("abc-123"),
        ),
        (
            r#"<Preferences PlexOnlineToken=" a&amp;b "></Preferences>"#,
            Some("a&b"),
        ),
        (r#"<Preferences MachineIdentifier="m"/>"#, None),
        (r#"<Preferences PlexOnlineToken="  "/>"#, None),
        (r#"<Other PlexOnlineToken="abc"/>"#, None),
        ("<!-- nothing -->", None),
        ("", None),
        (r#"<Preferences PlexOnlineToken="abc"#, None),
    ] {
        assert_eq!(
            online_token(preferences.as_bytes()).as_deref(),
            token,
            "{preferences}"
        );
    }
}

#[test]
fn preferences_are_claimable_where_they_name_the_identifier_plex_presents_to_plex_tv() {
    let claimable = super::claimable(
        br#"<Preferences ProcessedMachineIdentifier="abc" MachineIdentifier="m"/>"#,
    );
    assert_eq!(claimable.map(|one| one.client).as_deref(), Some("abc"));
    for refused in [
        &br#"<Preferences MachineIdentifier="m"/>"#[..],
        br#"<Other ProcessedMachineIdentifier="abc"/>"#,
        b"",
    ] {
        assert!(super::claimable(refused).is_none());
    }
}

#[test]
fn a_token_is_written_into_the_preferences_element_and_every_other_byte_kept() {
    let open = br#"<?xml version="1.0" encoding="utf-8"?>
<!-- kept -->
<Preferences A="1" PlexOnlineToken="old" ProcessedMachineIdentifier="c" B="&amp;"><Child x="y"/></Preferences>
"#;
    let written = super::claimable(open).map(|one| one.holding("new-token"));
    assert_eq!(
        written.as_deref().and_then(online_token).as_deref(),
        Some("new-token")
    );
    let text = String::from_utf8_lossy(written.as_deref().unwrap_or_default()).into_owned();
    assert!(
        text.starts_with(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- kept -->\n<Preferences "
        ),
        "{text}"
    );
    assert!(
        text.contains(r#"A="1""#) && text.contains(r#"B="&amp;""#),
        "{text}"
    );
    assert!(text.ends_with("<Child x=\"y\"/></Preferences>\n"), "{text}");
    assert_eq!(text.matches("PlexOnlineToken").count(), 1);

    let empty = br#"<Preferences ProcessedMachineIdentifier="c"/>"#;
    let written = super::claimable(empty).map(|one| one.holding("t"));
    assert_eq!(
        written.as_deref().and_then(online_token).as_deref(),
        Some("t")
    );
    assert!(written.is_some_and(|bytes| bytes.ends_with(b"/>")));
}
