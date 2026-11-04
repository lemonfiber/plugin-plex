//! The ages the certificates Plex's libraries hold are for, by the country the plex.tv
//! account that claimed Plex names (ARCH-R378). Plex keeps a certificate on each item as its
//! agent read it, prefixed with the country's code for every country but the United States,
//! and keeps no age for any of them, so the ages are this table's.

/// The country whose certificates Plex keeps without a prefix.
const UNPREFIXED: &str = "us";

const UNITED_STATES: &[(&str, u32)] = &[
    ("G", 0),
    ("TV-Y", 0),
    ("TV-G", 0),
    ("TV-Y7", 7),
    ("TV-Y7-FV", 7),
    ("PG", 10),
    ("TV-PG", 10),
    ("PG-13", 13),
    ("TV-14", 14),
    ("R", 17),
    ("TV-MA", 17),
    ("NC-17", 18),
];

const UNITED_KINGDOM: &[(&str, u32)] = &[
    ("U", 0),
    ("PG", 8),
    ("12A", 12),
    ("12", 12),
    ("15", 15),
    ("18", 18),
    ("R18", 18),
];

const GERMANY: &[(&str, u32)] = &[("0", 0), ("6", 6), ("12", 12), ("16", 16), ("18", 18)];

const FRANCE: &[(&str, u32)] = &[("U", 0), ("10", 10), ("12", 12), ("16", 16), ("18", 18)];

const NETHERLANDS: &[(&str, u32)] = &[
    ("AL", 0),
    ("6", 6),
    ("9", 9),
    ("12", 12),
    ("14", 14),
    ("16", 16),
    ("18", 18),
];

const CANADA: &[(&str, u32)] = &[("G", 0), ("PG", 8), ("14A", 14), ("18A", 18), ("R", 18)];

const AUSTRALIA: &[(&str, u32)] = &[
    ("G", 0),
    ("PG", 8),
    ("M", 15),
    ("MA15+", 15),
    ("R18+", 18),
    ("X18+", 18),
];

const IRELAND: &[(&str, u32)] = &[
    ("G", 0),
    ("PG", 8),
    ("12A", 12),
    ("15A", 15),
    ("16", 16),
    ("18", 18),
];

/// Each country this table holds, by its lowercase ISO 3166-1 code, with its certificates
/// and the age each is held at.
const BOARDS: &[(&str, &[(&str, u32)])] = &[
    (UNPREFIXED, UNITED_STATES),
    ("gb", UNITED_KINGDOM),
    ("de", GERMANY),
    ("fr", FRANCE),
    ("nl", NETHERLANDS),
    ("ca", CANADA),
    ("au", AUSTRALIA),
    ("ie", IRELAND),
];

/// One certificate as Plex keeps it on an item, with the name it is known by in its country
/// and the age this table holds it at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Rated {
    /// The value Plex keeps, prefix and all, which a share's filter names.
    pub(super) kept: String,
    /// The certificate's name in its country.
    pub(super) name: String,
    /// The age this table holds it at.
    pub(super) age: u32,
}

/// The certificates of `country`, or nothing where this table does not hold it.
fn board(country: &str) -> Option<&'static [(&'static str, u32)]> {
    BOARDS
        .iter()
        .find(|(code, _)| code.eq_ignore_ascii_case(country))
        .map(|(_, board)| *board)
}

/// Whether this table holds `country`.
pub(super) fn holds(country: &str) -> bool {
    board(country).is_some()
}

/// Every certificate of `country`, each as Plex keeps it on an item, or none where this
/// table does not hold the country.
pub(super) fn every(country: &str) -> Vec<Rated> {
    let Some(board) = board(country) else {
        return Vec::new();
    };
    let prefix = country.to_ascii_lowercase();
    board
        .iter()
        .map(|(name, age)| Rated {
            kept: if prefix == UNPREFIXED {
                (*name).to_owned()
            } else {
                format!("{prefix}/{name}")
            },
            name: (*name).to_owned(),
            age: *age,
        })
        .collect()
}

/// `kept`, a certificate as Plex keeps it, as a certificate of `country` with its age, or
/// nothing where it is another country's or this table does not age it.
pub(super) fn rated(country: &str, kept: &str) -> Option<Rated> {
    let board = board(country)?;
    let name = match kept.split_once('/') {
        Some((prefix, name)) if prefix.eq_ignore_ascii_case(country) => name,
        None if country.eq_ignore_ascii_case(UNPREFIXED) => kept,
        Some(_) | None => return None,
    };
    board
        .iter()
        .find(|(certificate, _)| certificate.eq_ignore_ascii_case(name))
        .map(|(certificate, age)| Rated {
            kept: kept.to_owned(),
            name: (*certificate).to_owned(),
            age: *age,
        })
}

#[cfg(test)]
mod tests;
