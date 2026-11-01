use std::os::unix::fs::PermissionsExt as _;

use super::Kept;

fn directory(name: &str) -> std::path::PathBuf {
    let at = std::env::temp_dir().join(format!("plex-kept-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&at);
    let _ = std::fs::create_dir_all(&at);
    at
}

#[test]
fn an_account_made_for_an_app_is_kept_until_it_is_gone_readable_by_its_owner_alone() {
    let at = directory("kept");
    let kept = Kept::in_directory(&at);

    assert_eq!(kept.read().map(|made| made.of("gate").len()), Ok(0));
    assert_eq!(kept.made("gate", "77"), Ok(()));
    assert_eq!(kept.made("decline", "78"), Ok(()));
    let made = kept.read().unwrap_or_default();
    assert!(made.holds("77") && made.holds("78") && !made.holds("79"));
    assert_eq!(made.of("gate").into_iter().collect::<Vec<_>>(), ["77"]);
    let permissions = std::fs::metadata(at.join(super::KEPT_FILE))
        .map(|meta| meta.permissions().mode() & 0o777)
        .ok();
    assert_eq!(permissions, Some(0o600));
    assert_eq!(kept.gone("77"), Ok(()));
    let made = kept.read().unwrap_or_default();
    assert!(!made.holds("77") && made.holds("78"));
    let _ = std::fs::remove_dir_all(&at);
}

#[test]
fn a_record_that_is_not_this_adapters_is_refused_rather_than_read_as_empty() {
    let at = directory("unreadable");
    let _ = std::fs::write(at.join(super::KEPT_FILE), "{\"apps\": [], \"other\": 1}");
    let kept = Kept::in_directory(&at);

    assert!(kept.read().is_err());
    assert!(kept.made("gate", "77").is_err());
    let _ = std::fs::remove_dir_all(&at);
}

#[test]
fn a_directory_that_cannot_be_written_is_said() {
    let kept = Kept::in_directory(std::path::Path::new("/nonexistent/plex-adapter"));

    assert!(kept.made("gate", "77").is_err());
}
