use lemonfiber_ports::service::Unrated;

use super::super::certificates::every;
use super::super::certificates::Rated;
use super::{
    closed, closing, opening, profile, profile_age, read, written, written_over, Profiled, CLOSED,
};

#[test]
fn a_filter_admitting_certificates_holds_back_unrated_content_at_the_highest_admitted_age() {
    assert_eq!(
        read("contentRating=gb/U%2Cgb/PG%2Cgb/12A"),
        (Some(12), Unrated::HeldBack)
    );
    assert_eq!(
        read("label=family|contentRating=G,PG"),
        (Some(10), Unrated::HeldBack)
    );
    assert_eq!(
        read("contentRating=de/16%2CR"),
        (Some(17), Unrated::HeldBack)
    );
}

#[test]
fn a_filter_refusing_certificates_lets_unrated_content_through_under_the_lowest_refused_age() {
    assert_eq!(
        read("contentRating!=gb/15%2Cgb/18%2CR"),
        (Some(14), Unrated::LetThrough)
    );
}

#[test]
fn no_filter_or_one_naming_nothing_this_table_ages_holds_no_limit() {
    assert_eq!(read(""), (None, Unrated::LetThrough));
    assert_eq!(read("label=family|broken"), (None, Unrated::LetThrough));
    assert_eq!(read("contentRating=zz/12"), (None, Unrated::HeldBack));
    assert_eq!(read("contentRating!=%2C"), (None, Unrated::LetThrough));
}

#[test]
fn holding_unrated_content_back_admits_only_certificates_at_or_under_the_limit_of_every_board() {
    let filter = written(Some(12), Unrated::HeldBack);
    let admitted: Vec<&str> = filter
        .trim_start_matches("contentRating=")
        .split("%2C")
        .collect();
    assert!(filter.starts_with("contentRating="), "{filter}");
    for rating in ["gb/12A", "gb/12", "PG", "de/12", "nl/12", "TV-Y7"] {
        assert!(admitted.contains(&rating), "{rating} in {filter}");
    }
    for rating in ["gb/15", "PG-13", "R", "de/16", "au/M", "TV-14"] {
        assert!(!admitted.contains(&rating), "{rating} in {filter}");
    }
}

#[test]
fn letting_unrated_content_through_refuses_every_boards_certificates_above_the_limit() {
    let filter = written(Some(12), Unrated::LetThrough);
    let refused: Vec<&str> = filter
        .trim_start_matches("contentRating!=")
        .split("%2C")
        .collect();
    assert!(filter.starts_with("contentRating!="), "{filter}");
    for rating in ["gb/15", "PG-13", "R", "NC-17", "de/16", "au/MA15+", "TV-MA"] {
        assert!(refused.contains(&rating), "{rating} in {filter}");
    }
    for rating in ["gb/12A", "PG", "de/12"] {
        assert!(!refused.contains(&rating), "{rating} in {filter}");
    }
    assert_eq!(written(Some(18), Unrated::LetThrough), "");
    assert_eq!(written(None, Unrated::LetThrough), "");
}

#[test]
fn holding_unrated_content_back_with_no_limit_admits_every_certificate_and_nothing_unrated() {
    let filter = written(None, Unrated::HeldBack);
    assert_eq!(
        filter
            .trim_start_matches("contentRating=")
            .split("%2C")
            .count(),
        every().len()
    );
}

#[test]
fn what_a_filter_is_written_as_reads_back_as_the_same_limit() {
    for limit in [0, 6, 8, 12, 15, 16] {
        let admitting = written(Some(limit), Unrated::HeldBack);
        assert_eq!(
            read(&admitting),
            (Some(limit), Unrated::HeldBack),
            "{limit}"
        );

        let refusing = written(Some(limit), Unrated::LetThrough);
        let (read_limit, read_unrated) = read(&refusing);
        assert_eq!(read_unrated, Unrated::LetThrough, "{refusing}");
        let read_limit = read_limit.unwrap_or_default();
        assert!(read_limit >= limit, "{refusing}");
        assert!(
            every()
                .iter()
                .all(|rated| rated.age <= limit || rated.age > read_limit),
            "{refusing}"
        );
    }
}

#[test]
fn a_closed_filter_admits_nothing_until_opened_and_keeps_its_limit_meanwhile() {
    let limited = "contentRating=gb/U%2Cgb/PG";
    let shut = closing(limited);
    assert_eq!(shut, format!("{limited}|{CLOSED}"));
    assert!(closed(&shut));
    assert_eq!(closing(&shut), shut);
    assert_eq!(read(&shut), read(limited));
    assert_eq!(opening(&shut), limited);
    assert!(!closed(&opening(&shut)));

    assert_eq!(closing(""), CLOSED);
    assert_eq!(opening(CLOSED), "");
    assert!(!closed(limited));
    assert!(!closed("label=lemonfiber-held-not"));
}

#[test]
fn a_managed_account_is_held_to_the_restriction_profile_nearest_at_or_under_its_limit() {
    for (limit, held) in [
        (0, Profiled::Unheld),
        (5, Profiled::Unheld),
        (6, Profiled::Held("little_kid")),
        (11, Profiled::Held("little_kid")),
        (12, Profiled::Held("older_kid")),
        (15, Profiled::Held("older_kid")),
        (16, Profiled::Held("teen")),
        (17, Profiled::Held("teen")),
        (18, Profiled::Unrestricted),
        (21, Profiled::Unrestricted),
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

#[test]
fn a_filter_that_would_admit_no_certificate_admits_nothing_rather_than_everything() {
    let only_adult = [Rated {
        kept: "gb/18".to_owned(),
        name: "18".to_owned(),
        age: 18,
    }];
    assert_eq!(
        written_over(&only_adult, Some(12), Unrated::HeldBack),
        CLOSED
    );
    assert_eq!(
        written_over(&only_adult, Some(12), Unrated::LetThrough),
        "contentRating!=gb/18"
    );
}
