//! immutable cosmetic data, reconstructed from retained source.
//! No actor/item population, mutable palette registry, query RNG or save pool.
use crate::appearance_state::{presentation_index_in_validated_palette, AppearanceKey};
use crate::creatures::BuildError;
use crate::ItemClass;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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
                #[serde(deserialize_with = "read_class")]
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
fn read_class<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ItemClass, D::Error> {
    use serde::de::IntoDeserializer;
    let value = String::deserialize(d)?;
    ItemClass::deserialize(value.into_deserializer())
}

/// Cosmetic only. The enclosing occurrence retains its real opaque handle and
/// operation metadata; this kind can never grant an item action or a stair exit.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectPresentation {
    pub name: Arc<str>,
    pub asset: Option<Arc<str>>,
    pub kind: ObjectPresentationKind,
}
impl ObjectPresentation {
    pub(crate) fn valid(&self) -> bool {
        name_valid(&self.name) && self.asset.as_deref().is_none_or(asset_valid)
    }
    pub(crate) fn encoded_size_bound(&self) -> usize {
        128usize
            .saturating_add(self.name.len().saturating_mul(2))
            .saturating_add(self.asset.as_ref().map_or(0, |a| a.len().saturating_mul(2)))
    }
}
impl<'de> Deserialize<'de> for ObjectPresentation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "read_name")]
            name: Arc<str>,
            #[serde(deserialize_with = "read_asset")]
            asset: Option<Arc<str>>,
            kind: ObjectPresentationKind,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            name: fields.name,
            asset: fields.asset,
            kind: fields.kind,
        })
    }
}
fn name_valid(value: &str) -> bool {
    value.len() <= crate::limits::DISPLAY_NAME_BYTES && !value.chars().any(char::is_control)
}
fn asset_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
fn read_name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Arc<str>, D::Error> {
    struct Text;
    impl serde::de::Visitor<'_> for Text {
        type Value = Arc<str>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded cosmetic text")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
            if name_valid(value) {
                Ok(Arc::from(value))
            } else {
                Err(E::custom("invalid cosmetic text"))
            }
        }
    }
    d.deserialize_str(Text)
}
fn read_asset<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Arc<str>>, D::Error> {
    struct Asset(Arc<str>);
    impl<'de> Deserialize<'de> for Asset {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let text = read_name(d)?;
            if asset_valid(&text) {
                Ok(Self(text))
            } else {
                Err(serde::de::Error::custom("invalid cosmetic asset"))
            }
        }
    }
    let value: Option<Asset> = crate::bounded::required_option(d)?;
    Ok(value.map(|asset| asset.0))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CreatureAppearance {
    pub name: Arc<str>,
    pub asset: Option<Arc<str>>,
}
impl CreatureAppearance {
    pub(crate) fn valid(&self) -> bool {
        name_valid(&self.name) && self.asset.as_deref().is_none_or(asset_valid)
    }
}
/// Already selected private cosmetic result. Seed is the constant neutral
/// cosmetic seed, never a palette ordinal, true subject seed, actor ID or the
/// reversible full selector hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreaturePresentation {
    pub name: Arc<str>,
    pub asset: Option<Arc<str>>,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationCatalog {
    creatures: Vec<CreatureAppearance>,
    objects: Vec<ObjectPresentation>,
}
impl PresentationCatalog {
    pub(crate) fn contains_object(&self, value: &ObjectPresentation) -> bool {
        self.objects.binary_search(value).is_ok()
    }
    /// Host preflights its validated source projection BEFORE producing vectors:
    /// distinct used pools, rows, shared text, directory and sort work. This
    /// constructor does not invent a lower source-admission cap. Native callers
    /// supply explicit owned rows; there is no untrusted palette decoder.
    /// Vecs remain behind this Arc: no second full Arc-slice row allocation.
    pub fn new(
        mut creatures: Vec<CreatureAppearance>,
        mut objects: Vec<ObjectPresentation>,
    ) -> Result<Arc<Self>, BuildError> {
        if creatures.iter().any(|row| !row.valid()) || objects.iter().any(|row| {
            !name_valid(&row.name)
                || row
                    .asset
                    .as_deref()
                    .is_some_and(|asset| !asset_valid(asset))
        }) {
            return Err(BuildError::InvalidDefinition);
        }
        // Host accounting reserves these constant fallback/stair rows too.
        if creatures.is_empty() {
            creatures
                .try_reserve_exact(1)
                .map_err(|_| BuildError::InvalidDefinition)?;
            creatures.push(CreatureAppearance {
                name: Arc::from(""),
                asset: None,
            });
        }
        if !objects
            .iter()
            .any(|row| matches!(row.kind, ObjectPresentationKind::Item { .. }))
        {
            objects
                .try_reserve_exact(2)
                .map_err(|_| BuildError::InvalidDefinition)?;
            objects.push(ObjectPresentation {
                name: Arc::from(""),
                asset: None,
                kind: ObjectPresentationKind::Item {
                    class: ItemClass::Misc,
                },
            });
        } else {
            objects
                .try_reserve_exact(1)
                .map_err(|_| BuildError::InvalidDefinition)?;
        }
        objects.push(ObjectPresentation {
            name: Arc::from(""),
            asset: None,
            kind: ObjectPresentationKind::Stairs,
        });
        creatures.sort_unstable();
        creatures.dedup();
        objects.sort_unstable();
        objects.dedup();
        Ok(Arc::new(Self { creatures, objects }))
    }
    pub(crate) fn creature(&self, key: AppearanceKey) -> CreaturePresentation {
        let index = presentation_index_in_validated_palette(key, self.creatures.len())
            .expect("validated nonempty immutable cosmetic palette");
        let row = &self.creatures[index];
        CreaturePresentation {
            name: row.name.clone(),
            asset: row.asset.clone(),
            seed: 0,
        }
    }
    pub(crate) fn object(&self, key: AppearanceKey) -> ObjectPresentation {
        let index = presentation_index_in_validated_palette(key, self.objects.len())
            .expect("validated nonempty immutable cosmetic palette");
        self.objects[index].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance_state::AppearanceChannel;
    fn key(subject: u64) -> AppearanceKey {
        AppearanceKey {
            seed: 42,
            observer: 1,
            subject,
            channel: AppearanceChannel::Creature,
            phase: 0,
        }
    }
    fn item(name: &str) -> ObjectPresentation {
        ObjectPresentation {
            name: Arc::from(name),
            asset: None,
            kind: ObjectPresentationKind::Item {
                class: ItemClass::Potion,
            },
        }
    }
    #[test]
    fn empty_categories_have_neutral_nonidentifying_choices_not_true_subject_fallback() {
        let palette = PresentationCatalog::new(vec![], vec![]).unwrap();
        assert_eq!(palette.creatures.len(), 1);
        assert_eq!(palette.objects.len(), 2);
        let first = palette.creature(key(2));
        let other = palette.creature(key(999));
        assert_eq!(first, other);
        assert!(first.name.is_empty());
        assert_eq!(first.asset, None);
        assert_eq!(first.seed, 0);
        assert!(palette.objects.iter().any(|row| row.kind
            == ObjectPresentationKind::Item {
                class: ItemClass::Misc
            }));
        assert!(palette
            .objects
            .iter()
            .any(|row| row.kind == ObjectPresentationKind::Stairs));
    }
    #[test]
    fn canonical_sort_and_dedup_ignore_source_alias_and_iteration_order() {
        let creature = CreatureAppearance {
            name: Arc::from("creature"),
            asset: Some(Arc::from("creature")),
        };
        let a = PresentationCatalog::new(
            vec![creature.clone(), creature.clone()],
            vec![item("blue"), item("amber"), item("blue")],
        )
        .unwrap();
        let b =
            PresentationCatalog::new(vec![creature], vec![item("amber"), item("blue")]).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.creature(key(2)).seed, 0);
        assert_eq!(a.creature(key(999)).seed, 0);
        assert_eq!(a.object(key(2)), b.object(key(2)));
    }
    #[test]
    fn wider_canonical_palette_is_not_rejected_by_old_public_primitive_cap() {
        let rows = (0..8192)
            .map(|index| item(&format!("appearance-{index:04}")))
            .collect();
        let palette = PresentationCatalog::new(vec![], rows).unwrap();
        assert_eq!(palette.objects.len(), 8193); // source rows plus cosmetic stairs
        assert!(palette.objects.contains(&palette.object(key(2))));
        // This is a native constructor/selector boundary, not qualification of
        // host source-envelope accounting or a claim of blanket OOM recovery.
    }
    #[test]
    fn cosmetic_record_is_closed_required_and_does_not_encode_navigation() {
        let appearance = item("amber");
        let value = serde_json::to_value(&appearance).unwrap();
        assert_eq!(
            serde_json::from_value::<ObjectPresentation>(value.clone()).unwrap(),
            appearance
        );
        for field in ["name", "asset", "kind"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<ObjectPresentation>(missing).is_err());
        }
        let mut extra = value.clone();
        extra["stairs_up"] = true.into();
        assert!(serde_json::from_value::<ObjectPresentation>(extra).is_err());
        let mut wrong = value.clone();
        wrong["kind"]["class"] = serde_json::json!({"potion":null});
        assert!(serde_json::from_value::<ObjectPresentation>(wrong).is_err());
        let mut wrong = value;
        wrong["kind"] = serde_json::json!({"kind":"stairs","class":"potion"});
        assert!(serde_json::from_value::<ObjectPresentation>(wrong).is_err());
    }
}
