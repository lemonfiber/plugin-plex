//! Plex's `Preferences.xml`: the one `Preferences` element, whose attributes hold the
//! server's identifier and, once an account claims it, the token Plex keeps for that
//! account.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer, XmlVersion};

const ROOT: &str = "Preferences";

const ONLINE_TOKEN: &str = "PlexOnlineToken";

const CLIENT: &str = "ProcessedMachineIdentifier";

/// The value of the attribute `name` on the `Preferences` element `preferences` opens
/// with, where it holds anything.
fn attribute(preferences: &[u8], name: &str) -> Option<String> {
    let mut reader = Reader::from_reader(preferences);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer).ok()? {
            Event::Start(element) | Event::Empty(element) => {
                if element.name().as_ref() != ROOT {
                    return None;
                }
                return element
                    .attributes()
                    .filter_map(Result::ok)
                    .find(|attribute| attribute.key.as_ref() == name)
                    .and_then(|attribute| attribute.normalized_value(XmlVersion::default()).ok())
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty());
            }
            Event::Eof => return None,
            _ => buffer.clear(),
        }
    }
}

/// The token `preferences` holds, or nothing where it holds none or is not Plex's
/// preferences.
pub(super) fn online_token(preferences: &[u8]) -> Option<String> {
    attribute(preferences, ONLINE_TOKEN)
}

/// Plex's preferences, read once, as a claim rewrites them: the identifier Plex presents
/// to plex.tv, under which a claim code is exchanged, and every byte around the
/// `Preferences` element, which the token is written into.
pub(super) struct Claimable {
    /// The identifier Plex presents to plex.tv as its client.
    pub(super) client: String,
    before: Vec<u8>,
    element: BytesStart<'static>,
    empty: bool,
    after: Vec<u8>,
}

/// `preferences` as a claim rewrites them, where they are Plex's and name the identifier
/// Plex presents to plex.tv.
pub(super) fn claimable(preferences: &[u8]) -> Option<Claimable> {
    let client = attribute(preferences, CLIENT)?;
    let mut reader = Reader::from_reader(preferences);
    let mut buffer = Vec::new();
    loop {
        let start = usize::try_from(reader.buffer_position()).ok()?;
        let (element, empty) = match reader.read_event_into(&mut buffer).ok()? {
            Event::Start(element) => (element.into_owned(), false),
            Event::Empty(element) => (element.into_owned(), true),
            _ => {
                buffer.clear();
                continue;
            }
        };
        let end = usize::try_from(reader.buffer_position()).ok()?;
        return Some(Claimable {
            client,
            before: preferences.get(..start)?.to_vec(),
            element,
            empty,
            after: preferences.get(end..)?.to_vec(),
        });
    }
}

impl Claimable {
    /// The preferences with `token` as the account's token, in place of any they held,
    /// every other attribute and byte as it was.
    pub(super) fn holding(&self, token: &str) -> Vec<u8> {
        let mut element = BytesStart::new("Preferences");
        for attribute in self.element.attributes().filter_map(Result::ok) {
            if attribute.key.as_ref() != ONLINE_TOKEN {
                element.push_attribute(attribute);
            }
        }
        element.push_attribute((ONLINE_TOKEN, token));
        let mut writer = Writer::new(self.before.clone());
        let event = if self.empty {
            Event::Empty(element)
        } else {
            Event::Start(element)
        };
        let _ = writer.write_event(event);
        let mut written = writer.into_inner();
        written.extend_from_slice(&self.after);
        written
    }
}

#[cfg(test)]
mod tests;
