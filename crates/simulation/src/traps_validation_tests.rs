use super::*;
use crate::checkpoint::{PersistError, SharedState};
use tor_world::{Extent, Location, Position, Region, RegionId, Trap, World};

fn fixture() -> (Game, Location) {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "trap validation".into(),
            bounds: Extent::new(5, 5, 2).unwrap(),
        })
        .unwrap();
    let at = Location {
        region: RegionId(1),
        position: Position { x: 2, y: 2, z: 0 },
    };
    (Game::new(world, 42, crate::test_creatures::content()), at)
}
fn rust() -> Trap {
    Trap {
        id: 1,
        dc: 12,
        avoidance_dc: None,
        kind: TrapKind::Rust {
            slot: "body_armor".into(),
            amount: 1,
        },
    }
}
#[test]
fn sleeping_gas_requires_finite_actor_active_incapacity() {
    use crate::content_ids::{ConditionFamilyId, ConditionId};
    use crate::rules::{
        ConditionDef, ConditionLifecycle, ConditionPreparation, ConditionRestriction,
        ConditionTiming, Eviction, Ruleset, Stacking,
    };
    let content = crate::test_creatures::content();
    let kind = TrapKind::SleepingGas {
        condition: "fixture_trap_sleep".into(),
        duration: 100,
        exposure: None,
    };
    for (timing, expected) in [
        (ConditionTiming::UntilRemoved, false),
        (ConditionTiming::expiring(), true),
    ] {
        let mut document = content.rules().document().clone();
        document.conditions.push(ConditionDef {
            id: ConditionId::new("fixture_trap_sleep").unwrap(),
            family: ConditionFamilyId::new("fixture_trap_sleep").unwrap(),
            lifecycle: ConditionLifecycle::Timed,
            name: "Trap sleep".into(),
            modifiers: vec![],
            stacking: Stacking::Keep,
            source_scoped: false,
            descriptors: vec![],
            capacity: Eviction::Reject,
            timing,
            restrictions: vec![ConditionRestriction::VoluntaryActions],
            preparation: ConditionPreparation::Pause,
        });
        let rules = Ruleset::validate(document).unwrap();
        assert_eq!(trap_kind_valid(&rules, &kind), expected);
        let mut missing_exposure = kind.clone();
        if let TrapKind::SleepingGas { exposure, .. } = &mut missing_exposure {
            *exposure = Some("missing_exposure".into());
        }
        assert!(!trap_kind_valid(&rules, &missing_exposure));
    }
}
#[test]
fn placement_rejects_unknown_effect_reference_without_mutating_world() {
    let (mut game, at) = fixture();
    let before = game.clone();
    for kind in [
        TrapKind::PoisonDart {
            attack_bonus: 0,
            exposure: "missing_exposure".into(),
        },
        TrapKind::Rust {
            slot: "missing_slot".into(),
            amount: 1,
        },
        TrapKind::SleepingGas {
            condition: "missing_condition".into(),
            duration: 100,
            exposure: None,
        },
    ] {
        assert!(game.place_trap(at, Trap { kind, ..rust() }).is_err());
        assert_eq!(game, before);
    }
    game.place_trap(at, rust()).unwrap();
    assert!(game.loaded_traps_valid());
}
#[test]
fn placement_cannot_reuse_an_unbuilt_regions_trap_identity() {
    let (mut game, at) = fixture();
    let mut identities = crate::RegionIdentities::default();
    identities.traps.insert(1);
    game.add_unbuilt_region(
        Region {
            id: RegionId(2),
            name: "reserved".into(),
            bounds: Extent::new(5, 5, 2).unwrap(),
        },
        true,
        identities,
    )
    .unwrap();
    let before = game.clone();
    assert!(game.place_trap(at, rust()).is_err());
    assert_eq!(game, before);
}
#[test]
fn loaded_and_detached_restore_reject_corrupted_trap_effect_references() {
    let (mut game, at) = fixture();
    game.place_trap(at, rust()).unwrap();
    // A full saved game requires at least one scheduled actor. Keep it off
    // the trap so setup does not manufacture a contact or discovery attempt.
    game.spawn_actor(
        Location {
            position: Position { x: 1, y: 1, z: 0 },
            ..at
        },
        std::num::NonZeroU64::new(100).unwrap(),
    )
    .unwrap();
    let mut valid_shared = SharedState::default();
    let valid_snapshot = game.checkpoint(&mut valid_shared);
    assert_eq!(
        Game::restore_checkpoint(valid_snapshot, &valid_shared, game.content().clone()).unwrap(),
        game
    );
    let record = game.clone().into_region_record(at.region).unwrap();
    let valid = crate::RegionRecordData::capture(&record);
    let bytes = serde_json::to_value(&valid).unwrap();
    assert!(valid.restore(game.content().clone()).is_ok());
    let mut corrupted = bytes;
    corrupted["world"]["traps"][0][1]["kind"]["slot"] = "missing_slot".into();
    let saved: crate::RegionRecordData = serde_json::from_value(corrupted).unwrap();
    assert_eq!(
        saved.restore(game.content().clone()),
        Err(PersistError::World)
    );

    let mut world = serde_json::to_value(&*game.world).unwrap();
    world["traps"][0][1]["kind"]["slot"] = "missing_slot".into();
    game.world = Shared::new(serde_json::from_value(world).unwrap());
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    assert_eq!(
        Game::restore_checkpoint(snapshot, &shared, game.content().clone()),
        Err(PersistError::World)
    );
}
