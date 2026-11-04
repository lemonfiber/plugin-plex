use lemonfiber_ports::service::Unrated;

use super::super::certificates::every;
use super::{profile, profile_age, read, written};

#[test]
fn a_filter_admitting_certificates_holds_back_unrated_content_at_the_highest_admitted_age() {
    assert_eq!(
        read("GB", "contentRating=gb/U%2Cgb/PG%2Cgb/12A"),
        (Some(12), Unrated::HeldBack)
    );
    assert_eq!(
        read("US", "label=family|contentRating=G,PG"),
        (Some(10), Unrated::HeldBack)
    );
}

#[test]
fn a_filter_refusing_certificates_lets_unrated_content_through_under_the_lowest_refused_age() {
    assert_eq!(
        read("GB", "contentRating!=gb/15%2Cgb/18%2Cgb/R18"),
        (Some(14), Unrated::LetThrough)
    );
}

#[test]
fn no_filter_or_one_naming_nothing_this_table_ages_holds_no_limit() {
    assert_eq!(read("GB", ""), (None, Unrated::LetThrough));
    assert_eq!(
        read("GB", "label=family|broken"),
        (None, Unrated::LetThrough)
    );
    assert_eq!(read("GB", "contentRating=de/12"), (None, Unrated::HeldBack));
    assert_eq!(
        read("GB", "contentRating!=%2C"),
        (None, Unrated::LetThrough)
    );
}

#[test]
fn holding_unrated_content_back_admits_only_the_certificates_at_or_under_the_limit() {
    assert_eq!(
        written(12, Unrated::HeldBack, &every("GB")).as_deref(),
        Some("contentRating=gb/U%2Cgb/PG%2Cgb/12A%2Cgb/12")
    );
    assert_eq!(written(12, Unrated::HeldBack, &[]), None);
}

#[test]
fn letting_unrated_content_through_refuses_the_certificates_above_the_limit() {
    assert_eq!(
        written(12, Unrated::LetThrough, &every("GB")).as_deref(),
        Some("contentRating!=gb/15%2Cgb/18%2Cgb/R18")
    );
    assert_eq!(
        written(18, Unrated::LetThrough, &every("GB")).as_deref(),
        Some("")
    );
}

#[test]
fn what_a_filter_is_written_as_reads_back_as_a_limit_holding_back_the_same_certificates() {
    let british = every("GB");
    for limit in [0, 8, 12, 15] {
        let admitting = written(limit, Unrated::HeldBack, &british).unwrap_or_default();
        assert_eq!(read("GB", &admitting), (Some(limit), Unrated::HeldBack));

        let refusing = written(limit, Unrated::LetThrough, &british).unwrap_or_default();
        let (read_limit, read_unrated) = read("GB", &refusing);
        assert_eq!(read_unrated, Unrated::LetThrough, "{refusing}");
        let read_limit = read_limit.unwrap_or_default();
        assert!(read_limit >= limit, "{refusing}");
        assert!(
            british
                .iter()
                .all(|rated| rated.age <= limit || rated.age > read_limit),
            "{refusing}"
        );
    }
}

#[test]
fn a_managed_account_is_held_to_the_restriction_profile_nearest_at_or_under_its_limit() {
    for (limit, held) in [
        (0, Some("little_kid")),
        (6, Some("little_kid")),
        (11, Some("little_kid")),
        (12, Some("older_kid")),
        (15, Some("older_kid")),
        (16, Some("teen")),
        (17, Some("teen")),
        (18, None),
        (21, None),
    ] {
        assert_eq!(profile(limit), held, "{limit}");
    }
}

#[test]
fn a_restriction_profile_is_read_as_the_highest_age_it_lets_watch() {
    assert_eq!(profile_age("little_kid"), Some(6));
    assert_eq!(profile_age("older_kid"), Some(12));
    assert_eq!(profile_age("teen"), Some(16));
    assert_eq!(profile_age("adult"), None);
}
