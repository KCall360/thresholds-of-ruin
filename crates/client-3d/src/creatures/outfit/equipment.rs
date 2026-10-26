//! Independently selected visual pieces composed over a wardrobe preset.
//! Offline recipes only; gameplay equipment remains server-owned.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum HeadItem {
    SteelHelm,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FootItem {
    LeatherBoots,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BodyItem {
    SteelBreastplate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BackItem {
    TravelCloak,
}

/// Selection is independent of the underlying preset. None restores that
/// preset's piece; it does not erase it or mutate the wearer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Equipment {
    pub head: Option<HeadItem>,
    pub feet: Option<FootItem>,
    pub back: Option<BackItem>,
    pub body: Option<BodyItem>,
}

/// Renderer-local resolution: preset replacements and independent outer layers
/// travel together. Geometry must consume this result rather than interpreting
/// raw selection presence as a particular item recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Composition {
    pub outfit: Outfit,
    pub body: Option<BodyItem>,
}

impl Composition {
    pub fn resolve(look: &Look) -> Self {
        let preset = look.outfit.unwrap_or_else(|| Outfit::native(look));
        Self {
            outfit: look.equipment.compose(preset),
            body: look.equipment.body,
        }
    }
}

impl Equipment {
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }

    pub fn select(&mut self, name: &str) -> Result<(), String> {
        match name {
            "helm" => self.head = Some(HeadItem::SteelHelm),
            "boots" => self.feet = Some(FootItem::LeatherBoots),
            "breastplate" => self.body = Some(BodyItem::SteelBreastplate),
            "cloak" => self.back = Some(BackItem::TravelCloak),
            _ => {
                return Err(format!(
                    "unknown equipment {name}; expected helm, boots, cloak or breastplate"
                ))
            }
        }
        Ok(())
    }

    pub(super) fn compose(self, mut base: Outfit) -> Outfit {
        if let Some(BodyItem::SteelBreastplate) = self.body {
            // An overlaid plate covers sewn bodice ornaments; independent
            // neck jewelry and straps are fitted onto the resolved exterior.
            base.torso_ornament = None;
        }
        if let Some(HeadItem::SteelHelm) = self.head {
            base.head = Headwear::Helm;
            if base.neck_drape.is_some_and(|drape| drape.covered_by_helm) {
                base.neck_drape = None;
            }
            // Covered decoration is suppressed in the resolved result only.
            base.trimmings.earrings = None;
        }
        if let Some(FootItem::LeatherBoots) = self.feet {
            base.feet = Feet::Boots(Boots {
                color: LEATHER * 0.75,
                tall: false,
                heel: false,
                buckles: true,
                cuffed: false,
            });
            base.trimmings.boot_tops = None;
        }
        if let Some(BackItem::TravelCloak) = self.back {
            base.cloak = Some(Cloak::new(
                Vec3::new(0.18, 0.29, 0.23),
                Vec3::new(0.48, 0.40, 0.25),
            ));
        }
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wearer_pigments_preserve_shading_and_fixed_color_identity() {
        let donor = Look::of(CreatureModel::Troll, 42);
        let recipe = Outfit::native(&donor);
        let recipient_skin = Vec3::new(0.72, 0.35, 0.23);
        assert_eq!(recipe.hands.resolve(donor.skin), donor.skin);
        assert_eq!(recipe.pants(donor.skin), donor.skin);
        assert_eq!(recipe.boot(donor.skin), donor.skin * 0.88);
        assert_eq!(recipe.hands.resolve(recipient_skin), recipient_skin);
        assert_eq!(recipe.pants(recipient_skin), recipient_skin);
        assert_eq!(recipe.boot(recipient_skin), recipient_skin * 0.88);
        let mut fixed = recipe;
        fixed.hands = Pigment::Fixed(donor.skin);
        fixed.legs = Legs::Fitted(Pigment::Fixed(donor.skin));
        fixed.feet = Feet::Bare(Pigment::Fixed(donor.skin));
        assert_eq!(fixed.hands.resolve(recipient_skin), donor.skin);
        assert_eq!(fixed.pants(recipient_skin), donor.skin);
        assert_eq!(fixed.boot(recipient_skin), donor.skin);
        assert_ne!(fixed, recipe, "cache identity retains pigment meaning");
    }

    #[test]
    fn native_chest_details_are_recipe_owned_and_covered_without_mutation() {
        for model in [
            CreatureModel::Human,
            CreatureModel::Mage,
            CreatureModel::Hobgoblin,
            CreatureModel::Zombie,
            CreatureModel::Vampire,
        ] {
            let look = Look::with_choice(
                model,
                42,
                Some(Variant {
                    woman: true,
                    ..default()
                }),
            );
            let original = Outfit::native(&look);
            let detail = original.torso_ornament.expect("native sewn detail");
            assert!(matches!(
                (model, detail),
                (CreatureModel::Human, TorsoOrnament::LacedFront { .. })
                    | (CreatureModel::Mage, TorsoOrnament::Placket { .. })
                    | (CreatureModel::Hobgoblin, TorsoOrnament::RivetedVest { .. })
                    | (CreatureModel::Zombie, TorsoOrnament::StitchedFront { .. })
                    | (CreatureModel::Vampire, TorsoOrnament::HighCollar { .. })
            ));
            let mut explicit = look.clone();
            explicit.outfit = Some(original);
            assert_eq!(Composition::resolve(&explicit).outfit, original);
            let mut equipment = Equipment::default();
            equipment.select("breastplate").unwrap();
            let covered = equipment.compose(original);
            assert!(covered.torso_ornament.is_none());
            assert_eq!(
                Outfit {
                    torso_ornament: Some(detail),
                    ..covered
                },
                original
            );
            assert_eq!(Equipment::default().compose(original), original);
        }
    }

    #[test]
    fn resolved_composition_keeps_outer_recipe_and_covered_details_together() {
        let base = Look::with_choice(
            CreatureModel::Human,
            17,
            Some(Variant {
                woman: true,
                ..default()
            }),
        )
        .dressed(Wardrobe::RoyalPrincess);
        let original = Composition::resolve(&base);
        assert!(original.body.is_none());
        assert!(original.outfit.torso_ornament.is_some());
        let mut equipment = Equipment::default();
        equipment.select("breastplate").unwrap();
        let equipped = base.clone().equipped(equipment);
        let resolved = Composition::resolve(&equipped);
        assert_eq!(resolved.body, Some(BodyItem::SteelBreastplate));
        assert!(resolved.outfit.torso_ornament.is_none());
        assert_eq!(resolved.outfit.skirt, original.outfit.skirt);
        assert_eq!(resolved.outfit.bodice, original.outfit.bodice);
        assert_eq!(
            Composition::resolve(&equipped.equipped(Equipment::default())),
            original
        );
    }

    #[test]
    fn equipment_overrides_only_its_regions_and_preserves_the_preset() {
        let look = Look::with_choice(
            CreatureModel::Human,
            17,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let preset = Outfit::from_wardrobe(Wardrobe::RoyalPrincess, &look);
        let mut equipment = Equipment::default();
        equipment.select("boots").unwrap();
        let boots = equipment.compose(preset);
        assert!(
            matches!(boots.feet, Feet::Boots(_)),
            "equipped boots replace sandals"
        );
        let mut restored = boots;
        restored.feet = preset.feet;
        restored.trimmings.boot_tops = preset.trimmings.boot_tops;
        assert_eq!(
            restored, preset,
            "footwear must not change the gown or crown"
        );
        equipment.select("helm").unwrap();
        let helm = equipment.compose(preset);
        assert_eq!(helm.head, Headwear::Helm);
        assert!(
            helm.trimmings.earrings.is_none(),
            "closed helm covers earrings"
        );
        assert_eq!(helm.trimmings.neck, preset.trimmings.neck);
        equipment.select("cloak").unwrap();
        let all = equipment.compose(preset);
        assert!(all.cloak.is_some());
        assert_eq!(all.skirt, preset.skirt);
        assert_eq!(all.bodice, preset.bodice);
        equipment.head = None;
        let removed_helm = equipment.compose(preset);
        assert_eq!(removed_helm.head, preset.head);
        assert_eq!(removed_helm.trimmings.earrings, preset.trimmings.earrings);
        assert_eq!(removed_helm.feet, all.feet);
        assert_eq!(removed_helm.cloak, all.cloak);
        assert_eq!(Equipment::default().compose(preset), preset);
    }

    #[test]
    fn helm_suppresses_only_the_hood_base_and_removal_restores_it() {
        let mut equipment = Equipment::default();
        equipment.select("helm").unwrap();
        for model in [
            CreatureModel::Mage,
            CreatureModel::Human,
            CreatureModel::Orc,
        ] {
            let look = Look::of(model, 17);
            let original = Composition::resolve(&look);
            let drape = original.outfit.neck_drape.expect("native neck drape");
            let equipped = look.clone().equipped(equipment);
            let resolved = Composition::resolve(&equipped);
            assert_eq!(
                resolved.outfit.neck_drape,
                (!drape.covered_by_helm).then_some(drape)
            );
            assert_eq!(
                Composition::resolve(&equipped.equipped(Equipment::default())),
                original
            );
        }
    }

    #[test]
    fn equipment_selection_is_order_independent_and_rejects_unknown_items() {
        let mut first = Equipment::default();
        let mut second = Equipment::default();
        for name in ["helm", "boots", "cloak", "breastplate"] {
            first.select(name).unwrap();
        }
        for name in ["breastplate", "cloak", "boots", "helm", "boots"] {
            second.select(name).unwrap();
        }
        assert_eq!(first, second);
        assert!(first.select("unimplemented-armor").is_err());
        assert_eq!(
            first, second,
            "invalid selection leaves equipment unchanged"
        );
        assert!(!first.is_empty());
    }
}

#[cfg(test)]
mod look_tests {
    use super::*;

    #[test]
    fn equipment_survives_redressing_and_removal_restores_canonical_identity() {
        let mut equipment = Equipment::default();
        for name in ["helm", "boots", "cloak", "breastplate"] {
            equipment.select(name).unwrap();
        }
        for model in [
            CreatureModel::Human,
            CreatureModel::Mage,
            CreatureModel::Orc,
        ] {
            for woman in [false, true] {
                let native = Look::with_choice(model, 17, Some(Variant { woman, ..default() }));
                for wardrobe in Wardrobe::ALL {
                    let preset = native.clone().dressed(wardrobe);
                    let composed = preset.clone().equipped(equipment);
                    let reversed = native.clone().equipped(equipment).dressed(wardrobe);
                    assert_eq!(
                        composed.key, reversed.key,
                        "selection order must not change cache identity"
                    );
                    assert_eq!(Outfit::worn(&composed), Outfit::worn(&reversed));
                    assert_ne!(composed.key, preset.key);
                    assert_eq!(composed.body_key.as_deref(), Some(native.key.as_str()));
                    assert_eq!(composed.v, native.v);
                    assert_eq!(composed.style, native.style);
                    assert_eq!(composed.scale, native.scale);
                    assert_eq!(composed.clone().equipped(equipment).key, composed.key);
                    let restored = composed.clone().equipped(Equipment::default());
                    assert_eq!(restored.key, preset.key);
                    assert_eq!(Outfit::worn(&restored), Outfit::worn(&preset));
                    let native_equipped = composed.dressed(Wardrobe::Native);
                    assert_eq!(native_equipped.equipment, equipment);
                    assert!(native_equipped.outfit.is_none());
                    assert_eq!(
                        native_equipped.equipped(Equipment::default()).key,
                        native.key
                    );
                }
            }
        }
    }

    #[test]
    fn wardrobe_catalog_preserves_browsing_fallback_and_exact_selection() {
        let man = Look::with_choice(CreatureModel::Human, 17, Some(Variant::default()));
        for wardrobe in Wardrobe::ALL {
            let expected = match wardrobe {
                Wardrobe::Princess | Wardrobe::RoyalPrincess => Wardrobe::Courtier,
                Wardrobe::SylvanDuelist => Wardrobe::Ranger,
                other => other,
            };
            assert_eq!(wardrobe.recipe_for(false), expected);
            assert_eq!(wardrobe.recipe_for(true), wardrobe);
            assert_eq!(
                Outfit::from_wardrobe(wardrobe, &man),
                Outfit::from_wardrobe(expected, &man)
            );
            assert_eq!(
                man.clone().composed(wardrobe, Equipment::default()).is_ok(),
                expected == wardrobe
            );
        }
    }

    #[test]
    fn checked_composition_is_exact_or_reports_unsupported_selection() {
        let empty = Equipment::default();
        let mut boots = empty;
        boots.select("boots").unwrap();
        let animal = Look::of(CreatureModel::Bat, 17);
        assert_eq!(
            animal
                .clone()
                .composed(Wardrobe::Native, empty)
                .unwrap()
                .key,
            animal.key
        );
        assert!(animal.clone().composed(Wardrobe::Native, boots).is_err());
        assert!(animal.composed(Wardrobe::Ranger, empty).is_err());
        let man = Look::with_choice(CreatureModel::Human, 17, Some(Variant::default()));
        for wardrobe in [
            Wardrobe::Princess,
            Wardrobe::RoyalPrincess,
            Wardrobe::SylvanDuelist,
        ] {
            assert!(man
                .clone()
                .composed(wardrobe, empty)
                .unwrap_err()
                .contains("female body"));
        }
        let woman = Look::with_choice(
            CreatureModel::Human,
            17,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let exact = woman
            .clone()
            .composed(Wardrobe::RoyalPrincess, boots)
            .unwrap();
        let expected = woman.dressed(Wardrobe::RoyalPrincess).equipped(boots);
        assert_eq!(exact.key, expected.key);
        assert_eq!(
            Composition::resolve(&exact),
            Composition::resolve(&expected)
        );
        assert_eq!(exact.equipment(), boots);
    }

    #[test]
    fn equipment_does_not_change_unsupported_body_recipes() {
        let mut equipment = Equipment::default();
        equipment.select("boots").unwrap();
        for model in [
            CreatureModel::Bat,
            CreatureModel::Guardian,
            CreatureModel::Yeti,
            CreatureModel::Centaur,
        ] {
            let look = Look::of(model, 17);
            let equipped = look.clone().equipped(equipment);
            assert_eq!(equipped.key, look.key);
            assert!(equipped.equipment.is_empty());
        }
    }
}

#[cfg(test)]
mod armor_tests {
    use super::*;
    #[test]
    fn breastplate_selection_preserves_compatible_garment_regions() {
        let mut equipment = Equipment::default();
        assert!(
            equipment.select("breastplate").is_ok(),
            "breastplate must be independently selectable"
        );
        let look = Look::with_choice(
            CreatureModel::Human,
            17,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        for wardrobe in [
            Wardrobe::RoyalPrincess,
            Wardrobe::Bard,
            Wardrobe::Ranger,
            Wardrobe::Native,
        ] {
            let preset = Outfit::from_wardrobe(wardrobe, &look);
            let resolved = equipment.compose(preset);
            assert_eq!(resolved.arms, preset.arms);
            assert_eq!(resolved.legs, preset.legs);
            assert_eq!(resolved.skirt, preset.skirt);
            assert_eq!(
                resolved.outer, preset.outer,
                "retain coat and peplum beneath armor"
            );
            assert_eq!(resolved.head, preset.head);
            assert_eq!(resolved.feet, preset.feet);
            assert_eq!(resolved.trimmings.neck, preset.trimmings.neck);
            assert!(
                resolved.torso_ornament.is_none(),
                "covered bodice ornaments must not pierce plate"
            );
            assert_eq!(Equipment::default().compose(preset), preset);
        }
    }
}
