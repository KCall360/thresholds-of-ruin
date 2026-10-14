//! Fixed immutable starter forms, not a spell-discovery or casting subsystem.
use super::{Modifier, Requirement, TalentDef};
use crate::content_ids::{AbilityId, FormId, TalentId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FormDef {
    pub id: FormId,
    pub name: String,
    pub ability: AbilityId,
}
impl<'de> Deserialize<'de> for FormDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: FormId,
            #[serde(deserialize_with = "display_name")]
            name: String,
            ability: AbilityId,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            id: fields.id,
            name: fields.name,
            ability: fields.ability,
        })
    }
}
impl<'de> Deserialize<'de> for TalentDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: TalentId,
            #[serde(deserialize_with = "display_name")]
            name: String,
            #[serde(
                deserialize_with = "super::strikes::bounded_vec::<_, _, { crate::limits::MODIFIERS_PER_SOURCE }>"
            )]
            prerequisites: Vec<TalentId>,
            requirement: Requirement,
            #[serde(
                deserialize_with = "super::strikes::bounded_vec::<_, _, { crate::limits::MODIFIERS_PER_SOURCE }>"
            )]
            modifiers: Vec<Modifier>,
            // Authoring shorthand only. The independent save schema requires it.
            #[serde(
                default,
                deserialize_with = "super::strikes::bounded_vec::<_, _, { crate::limits::FORMS_PER_TALENT }>"
            )]
            form_choices: Vec<FormId>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            id: fields.id,
            name: fields.name,
            prerequisites: fields.prerequisites,
            requirement: fields.requirement,
            modifiers: fields.modifiers,
            form_choices: fields.form_choices,
        })
    }
}
fn display_name<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    struct Text;
    impl serde::de::Visitor<'_> for Text {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(
                f,
                "a nonblank display name of at most {} bytes",
                crate::limits::DISPLAY_NAME_BYTES
            )
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
            if value.trim().is_empty()
                || value.len() > crate::limits::DISPLAY_NAME_BYTES
                || value.chars().any(char::is_control)
            {
                return Err(E::custom("invalid form or talent display name"));
            }
            Ok(value.to_owned())
        }
    }
    deserializer.deserialize_str(Text)
}
