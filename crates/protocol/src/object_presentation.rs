//! Cosmetic object appearances, separate from item and navigation authority.
use crate::ItemClass;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObjectPresentationKind {
    Item { class: ItemClass },
    Stairs,
}
impl<'de> Deserialize<'de> for ObjectPresentationKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Item {
                #[serde(deserialize_with = "class")]
                class: ItemClass,
            },
            Stairs {},
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Item { class } => Self::Item { class },
            Fields::Stairs {} => Self::Stairs,
        })
    }
}
fn class<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ItemClass, D::Error> {
    use serde::de::IntoDeserializer;
    let value = String::deserialize(d)?;
    ItemClass::deserialize(value.into_deserializer())
}

/// A sampled name and drawing recipe, never an action capability or identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObjectPresentation {
    pub name: String,
    pub asset: Option<String>,
    pub kind: ObjectPresentationKind,
}
impl ObjectPresentation {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.name.len() > 256 || self.name.chars().any(char::is_control) {
            return Err("invalid cosmetic name");
        }
        if self.asset.as_ref().is_some_and(|asset| !valid_asset(asset)) {
            return Err("invalid cosmetic asset");
        }
        Ok(())
    }
}
impl<'de> Deserialize<'de> for ObjectPresentation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "text")]
            name: String,
            #[serde(deserialize_with = "asset")]
            asset: Option<String>,
            kind: ObjectPresentationKind,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            name: f.name,
            asset: f.asset,
            kind: f.kind,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}
fn valid_asset(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
fn text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    struct Text;
    impl serde::de::Visitor<'_> for Text {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 256 UTF-8 bytes of cosmetic text")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if value.len() <= 256 && !value.chars().any(char::is_control) {
                Ok(value.to_owned())
            } else {
                Err(E::custom("invalid cosmetic text"))
            }
        }
    }
    d.deserialize_str(Text)
}
fn asset<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct Text(#[serde(deserialize_with = "text")] String);
    Option::<Text>::deserialize(d).map(|value| value.map(|value| value.0))
}
