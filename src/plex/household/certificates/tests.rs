use super::{aged, every, holds, rated, Rated};

fn named(country: &str, kept: &str) -> Option<(String, u32)> {
    rated(country, kept).map(|rated| (rated.name, rated.age))
}

#[test]
fn a_united_states_certificate_is_kept_unprefixed_and_aged() {
    assert_eq!(named("US", "PG-13"), Some(("PG-13".to_owned(), 13)));
    assert_eq!(named("us", "tv-ma"), Some(("TV-MA".to_owned(), 17)));
    assert_eq!(named("us", "us/R"), Some(("R".to_owned(), 17)));
}

#[test]
fn a_certificate_of_another_country_is_kept_behind_its_code_and_aged() {
    assert_eq!(named("GB", "gb/12A"), Some(("12A".to_owned(), 12)));
    assert_eq!(named("de", "DE/16"), Some(("16".to_owned(), 16)));
    assert_eq!(named("nl", "nl/AL"), Some(("AL".to_owned(), 0)));
}

#[test]
fn a_certificate_of_another_country_or_none_this_table_ages_is_not_rated() {
    assert_eq!(named("gb", "de/12"), None);
    assert_eq!(named("gb", "15"), None);
    assert_eq!(named("gb", "gb/XYZ"), None);
    assert_eq!(named("us", "gb/15"), None);
    assert_eq!(named("zz", "zz/12"), None);
}

#[test]
fn the_table_holds_the_countries_it_ages_certificates_for() {
    for country in ["US", "gb", "DE", "fr", "NL", "ca", "AU", "ie"] {
        assert!(holds(country), "{country}");
    }
    assert!(!holds("ZZ"));
    assert!(!holds(""));
}

#[test]
fn every_certificate_of_every_board_is_named_as_plex_keeps_it() {
    let every = every();
    for (kept, name, age) in [
        ("gb/12A", "12A", 12),
        ("NC-17", "NC-17", 18),
        ("de/16", "16", 16),
    ] {
        assert!(
            every.contains(&Rated {
                kept: kept.to_owned(),
                name: name.to_owned(),
                age,
            }),
            "{kept}"
        );
    }
    assert!(every.iter().all(|rated| !rated.kept.starts_with("us/")));
}

#[test]
fn a_certificate_of_any_board_is_aged_by_its_prefix_or_as_american_without_one() {
    let age = |kept: &str| aged(kept).map(|rated| rated.age);
    assert_eq!(age("gb/15"), Some(15));
    assert_eq!(age("R"), Some(17));
    assert_eq!(age("au/MA15+"), Some(15));
    assert_eq!(age("zz/15"), None);
    assert_eq!(age("Unrated"), None);
}
