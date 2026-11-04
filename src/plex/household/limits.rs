//! What an age limit comes to on Plex (ARCH-R378): on a share, a filter of the certificates
//! its libraries hold; on a managed Home account, one of Plex's restriction profiles.

use lemonfiber_ports::service::Unrated;

use super::certificates::{self, Rated};

/// What a share's filter names its certificates by.
const RATING: &str = "contentRating";

/// What a share's filter names the certificates it refuses by.
const NOT_RATING: &str = "contentRating!";

/// How a share's filter separates its parts.
const PARTS: char = '|';

/// How a share's filter separates the values of one part, as plex.tv keeps it.
const VALUES: &str = "%2C";

/// Plex's restriction profiles, each with the highest age it lets a managed account watch.
const PROFILES: &[(&str, u32)] = &[("little_kid", 6), ("older_kid", 12), ("teen", 16)];

/// The age limit from which a managed account is held to no restriction profile.
const UNRESTRICTED_FROM: u32 = 18;

/// A share's filter of certificates, as plex.tv keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Filter {
    /// Only these certificates, and nothing unrated.
    Admitting(Vec<String>),
    /// Everything but these certificates, unrated included.
    Refusing(Vec<String>),
    /// Everything.
    Open,
}

fn filter(kept: &str) -> Filter {
    for part in kept.split(PARTS) {
        let Some((key, values)) = part.split_once('=') else {
            continue;
        };
        let named: Vec<String> = values
            .split(VALUES)
            .flat_map(|value| value.split(','))
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();
        match key {
            RATING => return Filter::Admitting(named),
            NOT_RATING => return Filter::Refusing(named),
            _ => {}
        }
    }
    Filter::Open
}

/// The age limit and the choice about unrated content a share's filter `kept` holds, read
/// against `country`'s certificates.
pub(super) fn read(country: &str, kept: &str) -> (Option<u32>, Unrated) {
    let ages = |named: &[String]| -> Vec<u32> {
        named
            .iter()
            .filter_map(|one| certificates::rated(country, one))
            .map(|rated| rated.age)
            .collect()
    };
    match filter(kept) {
        Filter::Admitting(named) => (ages(&named).into_iter().max(), Unrated::HeldBack),
        Filter::Refusing(named) => (
            ages(&named)
                .into_iter()
                .min()
                .map(|lowest| lowest.saturating_sub(1)),
            Unrated::LetThrough,
        ),
        Filter::Open => (None, Unrated::LetThrough),
    }
}

/// The filter holding a share to `age_limit` over the certificates `named`: admitting only
/// those at or under it where unrated content is held back, or refusing those above it
/// where it is let through. Nothing where unrated content is held back and no certificate
/// is at or under the limit, since an empty filter admits everything.
pub(super) fn written(age_limit: u32, unrated: Unrated, named: &[Rated]) -> Option<String> {
    let picked = |admitted: bool| -> Vec<&str> {
        named
            .iter()
            .filter(|rated| (rated.age <= age_limit) == admitted)
            .map(|rated| rated.kept.as_str())
            .collect()
    };
    match unrated {
        Unrated::HeldBack => {
            let admitted = picked(true);
            (!admitted.is_empty()).then(|| format!("{RATING}={}", admitted.join(VALUES)))
        }
        Unrated::LetThrough => {
            let refused = picked(false);
            Some(if refused.is_empty() {
                String::new()
            } else {
                format!("{NOT_RATING}={}", refused.join(VALUES))
            })
        }
    }
}

/// The restriction profile nearest at or under `age_limit`, the youngest where none is, or
/// none from [`UNRESTRICTED_FROM`].
pub(super) fn profile(age_limit: u32) -> Option<&'static str> {
    if age_limit >= UNRESTRICTED_FROM {
        return None;
    }
    PROFILES
        .iter()
        .rev()
        .find(|(_, age)| *age <= age_limit)
        .or_else(|| PROFILES.first())
        .map(|(name, _)| *name)
}

/// The highest age the restriction profile `name` lets a managed account watch.
pub(super) fn profile_age(name: &str) -> Option<u32> {
    PROFILES
        .iter()
        .find(|(profile, _)| *profile == name)
        .map(|(_, age)| *age)
}

#[cfg(test)]
mod tests;
