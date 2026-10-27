//! Bounded composition sampling. Fixed witnesses survive every seed change.
use super::*;

pub(super) const MODELS: [CreatureModel; 11] = [
    CreatureModel::Human,
    CreatureModel::Mage,
    CreatureModel::RedMage,
    CreatureModel::Sentinel,
    CreatureModel::Vampire,
    CreatureModel::Zombie,
    CreatureModel::Troll,
    CreatureModel::Leprechaun,
    CreatureModel::Nymph,
    CreatureModel::Medusa,
    CreatureModel::Phantom,
];
const RANDOM_CASES: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Case {
    pub model: CreatureModel,
    pub wardrobe: Wardrobe,
    pub seed: u64,
    pub variant: Variant,
    pub equipment: Equipment,
}

// SplitMix64: an explicit algorithm keeps replay independent of library defaults.
fn draw(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

pub(super) fn plan(mut seed: u64) -> Vec<Case> {
    let mut cases = Vec::new();
    for model in MODELS {
        for woman in [false, true] {
            for wardrobe in [Wardrobe::Native, Wardrobe::Knight] {
                cases.push(Case {
                    model,
                    wardrobe,
                    seed: 2,
                    equipment: Equipment::default(),
                    variant: Variant { woman, ..default() },
                });
            }
        }
    }
    cases.push(Case {
        model: CreatureModel::Sentinel,
        wardrobe: Wardrobe::Courtier,
        seed: 2,
        equipment: Equipment::default(),
        variant: Variant {
            woman: true,
            ..default()
        },
    });
    for woman in [false, true] {
        cases.push(Case {
            model: CreatureModel::Medusa,
            wardrobe: Wardrobe::Commoner,
            seed: 2,
            equipment: Equipment::default(),
            variant: Variant { woman, ..default() },
        });
    }
    // These conflict witnesses remain fixed as the random seed changes.
    for (model, wardrobe, woman, items) in [
        (
            CreatureModel::Human,
            Wardrobe::SylvanDuelist,
            true,
            &["breastplate"][..],
        ),
        (
            CreatureModel::Nymph,
            Wardrobe::RoyalPrincess,
            true,
            &["helm"][..],
        ),
        (CreatureModel::Mage, Wardrobe::Native, false, &["cloak"][..]),
        (
            CreatureModel::RedMage,
            Wardrobe::Native,
            true,
            &["boots"][..],
        ),
        (
            CreatureModel::Troll,
            Wardrobe::Courtier,
            true,
            &["helm", "boots", "breastplate", "cloak"][..],
        ),
    ] {
        let mut equipment = Equipment::default();
        for item in items {
            equipment.select(item).unwrap();
        }
        cases.push(Case {
            model,
            wardrobe,
            seed: 7,
            variant: Variant { woman, ..default() },
            equipment,
        });
    }
    for _ in 0..RANDOM_CASES {
        let model = MODELS[draw(&mut seed) as usize % MODELS.len()];
        let individual = draw(&mut seed);
        let variant = Variant {
            woman: draw(&mut seed) & 1 != 0,
            build: (draw(&mut seed) % 3) as u8,
            bust: (draw(&mut seed) % 5) as u8,
            bust_form: (draw(&mut seed) % 6) as u8,
            bust_lean: (draw(&mut seed) % 3) as u8,
            figure: (draw(&mut seed) % 3) as u8,
            ..default()
        };
        let eligible: Vec<_> = Wardrobe::ALL
            .into_iter()
            .filter(|wardrobe| wardrobe.recipe_for(variant.woman) == *wardrobe)
            .collect();
        let wardrobe = eligible[draw(&mut seed) as usize % eligible.len()];
        let mask = draw(&mut seed);
        let mut equipment = Equipment::default();
        for (bit, item) in ["helm", "boots", "breastplate", "cloak"]
            .into_iter()
            .enumerate()
        {
            if mask & (1 << bit) != 0 {
                equipment.select(item).unwrap();
            }
        }
        cases.push(Case {
            model,
            wardrobe,
            seed: individual,
            equipment,
            variant,
        });
    }
    cases
}

#[test]
fn composition_sampling_is_bounded_replayable_and_preserves_conflict_witnesses() {
    let first = plan(123);
    assert_eq!(first, plan(123));
    assert_eq!(first.len(), 84);
    assert_eq!(&first[..52], &plan(999)[..52]);
    assert_ne!(&first[52..], &plan(999)[52..]);
    for model in MODELS {
        for woman in [false, true] {
            assert!(first.iter().any(|c| c.model == model
                && c.variant.woman == woman
                && c.wardrobe == Wardrobe::Knight));
            assert!(first.iter().any(|c| c.model == model
                && c.variant.woman == woman
                && c.wardrobe == Wardrobe::Native));
        }
    }
    assert!(first.iter().any(|c| c.model == CreatureModel::Sentinel
        && c.variant.woman
        && c.wardrobe == Wardrobe::Courtier));
    assert!(first[52..]
        .iter()
        .any(|c| c.variant.build != 1 || c.variant.bust != 2));
}

#[test]
fn sampled_compositions_exercise_independent_equipment_and_conflicts() {
    let cases = plan(123);
    assert!(
        cases.iter().all(|c| c.variant.woman
            || !matches!(
                c.wardrobe,
                Wardrobe::Princess | Wardrobe::RoyalPrincess | Wardrobe::SylvanDuelist
            )),
        "samples must request valid compositions, not silently test a fallback"
    );
    assert!(cases[52..].iter().any(|c| !c.equipment.is_empty()));
    assert_ne!(
        cases[52..].iter().map(|c| c.equipment).collect::<Vec<_>>(),
        plan(999)[52..]
            .iter()
            .map(|c| c.equipment)
            .collect::<Vec<_>>()
    );
    assert!(
        cases.iter().any(|c| c.variant.woman
            && c.wardrobe == Wardrobe::SylvanDuelist
            && c.equipment.body.is_some()
            && c.equipment.head.is_none()),
        "retain exposed-hair breastplate conflict"
    );
    assert!(
        cases
            .iter()
            .any(|c| c.wardrobe == Wardrobe::RoyalPrincess && c.equipment.head.is_some()),
        "retain crown/helm replacement conflict"
    );
    for select in ["helm", "boots", "breastplate", "cloak"] {
        let mut item = Equipment::default();
        item.select(select).unwrap();
        assert!(
            cases.iter().any(|c| c.equipment == item),
            "missing isolated {select}"
        );
    }
    assert!(
        cases.iter().any(|c| c.equipment.head.is_some()
            && c.equipment.body.is_some()
            && c.equipment.feet.is_some()
            && c.equipment.back.is_some()),
        "all slots compose together"
    );
}
