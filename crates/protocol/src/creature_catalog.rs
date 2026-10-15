//! Private, bounded discovery of the server's named creature spawn recipes.
use serde::{Deserialize, Serialize};

pub const MAX_CREATURE_CATALOG_ENTRIES: usize = 32;
pub const MAX_CREATURE_SELECTOR_BYTES: usize = 768;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureCatalogEntry {
    pub selector: String,
    pub name: String,
    pub details: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureCatalogPage {
    /// None identifies an authored base, without an inferred floor depth.
    pub depth: Option<u32>,
    pub entries: Vec<CreatureCatalogEntry>,
    /// The final selector on this page, when more entries remain.
    pub next: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCreatureCatalog;
impl std::fmt::Display for InvalidCreatureCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid creature catalog")
    }
}
impl std::error::Error for InvalidCreatureCatalog {}

fn text_valid(value: &str, max_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control)
}
fn selector_valid(value: &str) -> bool {
    text_valid(value, MAX_CREATURE_SELECTOR_BYTES) && !value.chars().any(char::is_whitespace)
}
impl CreatureCatalogEntry {
    fn valid(&self) -> bool {
        selector_valid(&self.selector)
            && text_valid(&self.name, 80)
            && text_valid(&self.details, 400)
    }
}
impl CreatureCatalogPage {
    /// Validate directly constructed values as well as decoded server replies.
    pub fn validate(&self) -> Result<(), InvalidCreatureCatalog> {
        let valid = self.depth != Some(0)
            && self.entries.len() <= MAX_CREATURE_CATALOG_ENTRIES
            && self.entries.iter().all(CreatureCatalogEntry::valid)
            && self
                .entries
                .windows(2)
                .all(|pair| pair[0].selector < pair[1].selector)
            && self.next.as_ref().is_none_or(|next| {
                self.entries
                    .last()
                    .is_some_and(|entry| &entry.selector == next)
            });
        valid.then_some(()).ok_or(InvalidCreatureCatalog)
    }
}

fn text<'de, D: serde::Deserializer<'de>, const N: usize>(d: D) -> Result<String, D::Error> {
    struct Visitor<const N: usize>;
    impl<const N: usize> serde::de::Visitor<'_> for Visitor<N> {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "nonempty text of at most {N} UTF-8 bytes without controls"
            )
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if !text_valid(value, N) {
                return Err(E::custom("invalid creature catalog text"));
            }
            Ok(value.to_owned())
        }
    }
    d.deserialize_str(Visitor::<N>)
}
fn selector<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = text::<D, MAX_CREATURE_SELECTOR_BYTES>(d)?;
    if !selector_valid(&value) {
        return Err(serde::de::Error::custom("invalid creature selector"));
    }
    Ok(value)
}
fn optional_selector<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct Selector(#[serde(deserialize_with = "selector")] String);
    Option::<Selector>::deserialize(d).map(|value| value.map(|value| value.0))
}
fn depth<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    let value = Option::<u32>::deserialize(d)?;
    if value == Some(0) {
        return Err(serde::de::Error::custom(
            "creature catalog depth must be positive",
        ));
    }
    Ok(value)
}
impl<'de> Deserialize<'de> for CreatureCatalogEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "selector")]
            selector: String,
            #[serde(deserialize_with = "text::<_,80>")]
            name: String,
            #[serde(deserialize_with = "text::<_,400>")]
            details: String,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            selector: fields.selector,
            name: fields.name,
            details: fields.details,
        })
    }
}
impl<'de> Deserialize<'de> for CreatureCatalogPage {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "depth")]
            depth: Option<u32>,
            #[serde(
                deserialize_with = "crate::bounded::bounded::<_,_,MAX_CREATURE_CATALOG_ENTRIES>"
            )]
            entries: Vec<CreatureCatalogEntry>,
            #[serde(deserialize_with = "optional_selector")]
            next: Option<String>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            depth: fields.depth,
            entries: fields.entries,
            next: fields.next,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

pub(crate) fn read_reply<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(crate::ReplyContext, String, Box<CreatureCatalogPage>), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        context: crate::ReplyContext,
        #[serde(deserialize_with = "text::<_,128>")]
        request_id: String,
        page: Box<CreatureCatalogPage>,
    }
    let fields: Fields = crate::bounded::named_map(d)?;
    Ok((fields.context, fields.request_id, fields.page))
}
