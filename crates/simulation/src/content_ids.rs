//! Stable, typed content identities. Runtime loading interns strings; persistence uses text.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= crate::limits::ID_BYTES
        && value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.')
        })
}
macro_rules! ids {
    ($($name:ident),+ $(,)?) => {$ (
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(Arc<str>);
        impl $name {
            pub fn new(value: impl AsRef<str>) -> Result<Self, InvalidContentId> {
                let value = value.as_ref();
                if !valid_id(value) { return Err(InvalidContentId(value.into())); }
                Ok(Self(Arc::from(value)))
            }
            pub fn as_str(&self) -> &str { &self.0 }
            pub(crate) fn intern(&mut self, pool: &mut Interner) { self.0 = pool.intern(&self.0); }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(|e| serde::de::Error::custom(format!("invalid content id: {}", e.0)))
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.as_str().fmt(f) }
        }
    )+};
}
ids!(
    RulesetId,
    AbilityId,
    TalentId,
    TypeId,
    SubtypeId,
    ClassId,
    ConditionId,
    DescriptorId,
    FlagId,
    SpeciesId,
    TemplateId
);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidContentId(pub String);
#[derive(Default)]
pub(crate) struct Interner(BTreeMap<String, Arc<str>>);
impl Interner {
    fn intern(&mut self, value: &str) -> Arc<str> {
        self.0
            .entry(value.into())
            .or_insert_with(|| Arc::from(value))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_serialize_as_stable_strings() {
        let id = TalentId::new("custom_talent").unwrap();
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"custom_talent\"");
        assert_eq!(
            serde_json::from_str::<TalentId>("\"custom_talent\"").unwrap(),
            id
        );
        assert!(TalentId::new("").is_err());
        assert!(serde_json::from_str::<TalentId>("\"bad id\"").is_err());
        assert!(TalentId::new("wrong space").is_err());
        assert!(TalentId::new("x".repeat(crate::limits::ID_BYTES + 1)).is_err());
    }
    #[test]
    fn interning_reuses_one_allocation_even_across_content_categories() {
        let mut ability = AbilityId::new("shared").unwrap();
        let mut talent = TalentId::new("shared").unwrap();
        let mut pool = Interner::default();
        ability.intern(&mut pool);
        talent.intern(&mut pool);
        assert!(Arc::ptr_eq(&ability.0, &talent.0));
    }
}
