//! What an age limit comes to on Plex (ARCH-R378): on a share, a filter of certificates; on a
//! managed Home account, one of Plex's restriction profiles. Every filter is written over
//! every certificate the table ages, whichever country it is of, so content rated by another
//! country's board is held to the same ages.

use lemonfiber_ports::service::Unrated;

use super::certificates::{self, Rated};

/// What a share's filter names the certificates it admits by.
const RATING: &str = "contentRating";

/// What a share's filter names the certificates it refuses by.
const NOT_RATING: &str = "contentRating!";

/// How a share's filter separates its parts, each of which an item must meet.
const PARTS: &str = "|";

/// How a share's filter separates the values of one part, as plex.tv keeps it.
const VALUES: &str = "%2C";

/// The part that admits nothing: a label no item carries. A share holding it is closed.
pub(super) const CLOSED: &str = "label=lemonfiber-held";

/// Plex's restriction profiles, each with the highest age it lets a managed account watch.
const PROFILES: &[(&str, u32)] = &[("little_kid", 6), ("older_kid", 12), ("teen", 16)];

/// The age limit from which a managed account is held to no restriction profile.
const UNRESTRICTED_FROM: u32 = 18;

/// What an age limit comes to on a managed Home account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Profiled {
    /// The restriction profile nearest at or under the limit.
    Held(&'static str),
    /// No restriction profile: the limit holds nothing back.
    Unrestricted,
    /// Under every restriction profile Plex has, so none holds the limit.
    Unheld,
}

fn parts(kept: &str) -> impl Iterator<Item = &str> {
    kept.split(PARTS).filter(|part| !part.is_empty())
}

fn named(values: &str) -> Vec<&str> {
    values
        .split(VALUES)
        .flat_map(|value| value.split(','))
        .filter(|value| !value.is_empty())
        .collect()
}

/// The age limit and the choice about unrated content a share's filter `kept` holds.
pub(super) fn read(kept: &str) -> (Option<u32>, Unrated) {
    let ages = |values: &str| -> Vec<u32> {
        named(values)
            .into_iter()
            .filter_map(certificates::aged)
            .map(|rated| rated.age)
            .collect()
    };
    for part in parts(kept) {
        match part.split_once('=') {
            Some((RATING, values)) => {
                return (ages(values).into_iter().max(), Unrated::HeldBack);
            }
            Some((NOT_RATING, values)) => {
                return (
                    ages(values)
                        .into_iter()
                        .min()
                        .map(|lowest| lowest.saturating_sub(1)),
                    Unrated::LetThrough,
                );
            }
            _ => {}
        }
    }
    (None, Unrated::LetThrough)
}

/// Whether a share's filter `kept` admits nothing.
pub(super) fn closed(kept: &str) -> bool {
    parts(kept).any(|part| part == CLOSED)
}

/// `kept` admitting nothing, whatever else it holds.
pub(super) fn closing(kept: &str) -> String {
    if closed(kept) {
        return kept.to_owned();
    }
    parts(kept).chain([CLOSED]).collect::<Vec<_>>().join(PARTS)
}

/// `kept` without what closes it.
pub(super) fn opening(kept: &str) -> String {
    parts(kept)
        .filter(|part| *part != CLOSED)
        .collect::<Vec<_>>()
        .join(PARTS)
}

/// The filter holding a share to `age_limit`, or to no limit, and to `unrated`: admitting only
/// the certificates at or under it where unrated content is held back, or refusing those
/// above it where it is let through. Nothing where unrated content is held back and no
/// certificate is at or under the limit.
pub(super) fn written(age_limit: Option<u32>, unrated: Unrated) -> Option<String> {
    let every: Vec<Rated> = certificates::every();
    let picked = |admitted: bool| -> Vec<String> {
        every
            .iter()
            .filter(|rated| age_limit.is_none_or(|limit| rated.age <= limit) == admitted)
            .map(|rated| rated.kept.clone())
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

/// What `age_limit` comes to on a managed Home account: the restriction profile nearest at or
/// under it, none from [`UNRESTRICTED_FROM`], and unheld under the youngest.
pub(super) fn profile(age_limit: u32) -> Profiled {
    if age_limit >= UNRESTRICTED_FROM {
        return Profiled::Unrestricted;
    }
    PROFILES
        .iter()
        .rev()
        .find(|(_, age)| *age <= age_limit)
        .map_or(Profiled::Unheld, |(name, _)| Profiled::Held(name))
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
