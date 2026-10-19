use super::*;
use serde_json::json;

fn id(value: &str) -> EquipmentCapacityId {
    EquipmentCapacityId::new(value).unwrap()
}
fn capacities(grips: u16) -> EquipmentCapacities {
    EquipmentCapacities::new(BTreeMap::from([(id("grip"), grips)])).unwrap()
}

#[test]
fn aggregate_minimum_rejects_impossible_assignments() {
    assert!(capacity_deficits(&capacities(2), [(id("grip"), 2, 2), (id("grip"), 2, 2)]).is_none());
    assert!(capacity_deficits(
        &capacities(2),
        [(id("grip"), 1, 1), (id("grip"), 1, 1), (id("grip"), 1, 1)]
    )
    .is_none());
}

#[test]
fn current_demand_can_exceed_structural_minimum_after_expertise_loss() {
    let deficits =
        capacity_deficits(&capacities(2), [(id("grip"), 1, 2), (id("grip"), 1, 1)]).unwrap();
    assert_eq!(
        deficits[&id("grip")],
        EquipmentCapacityDeficit {
            available: 2,
            required: 3
        }
    );
    assert_eq!(deficits[&id("grip")].missing(), 1);
    assert!(
        capacity_deficits(&capacities(2), [(id("grip"), 1, 1), (id("grip"), 1, 1)])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn resource_pools_are_independent_and_have_no_implicit_capacity() {
    let pools =
        EquipmentCapacities::new(BTreeMap::from([(id("grip"), 1), (id("mount"), 3)])).unwrap();
    let deficits = capacity_deficits(&pools, [(id("grip"), 1, 2), (id("mount"), 3, 3)]).unwrap();
    assert_eq!(deficits.len(), 1);
    assert!(deficits.contains_key(&id("grip")));
    assert!(capacity_deficits(&pools, [(id("unknown"), 1, 1)]).is_none());
    assert!(capacity_deficits(&capacities(0), [(id("grip"), 1, 1)]).is_none());
    assert!(capacity_deficits(&EquipmentCapacities::default(), [])
        .unwrap()
        .is_empty());
    assert!(
        capacity_deficits(&capacities(3), (0..3).map(|_| (id("grip"), 1, 1)))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn malformed_and_unbounded_demands_are_rejected() {
    let large = capacity_deficits(&capacities(64), [(id("grip"), 64, 128)]).unwrap();
    assert_eq!(large[&id("grip")].missing(), 64);
    for demand in [(0, 1), (2, 1)] {
        assert!(capacity_deficits(&capacities(64), [(id("grip"), demand.0, demand.1)]).is_none());
    }
    assert!(capacity_deficits(&capacities(64), (0..65).map(|_| (id("grip"), 1, 1))).is_none());
    assert!(EquipmentCapacities::new(BTreeMap::from([(id("grip"), 65)])).is_none());
    let many = (0..65).map(|i| (id(&format!("pool_{i}")), 1)).collect();
    assert!(EquipmentCapacities::new(many).is_none());
}

#[test]
fn capacity_map_requires_unique_bounded_named_entries() {
    for raw in [
        r#"{"grip":1,"grip":2}"#,
        r#"{"grip":65}"#,
        r#"{"bad id":1}"#,
        "[1,2]",
    ] {
        assert!(
            serde_json::from_str::<EquipmentCapacities>(raw).is_err(),
            "{raw}"
        );
    }
    let pools = capacities(3);
    assert_eq!(
        serde_json::from_value::<EquipmentCapacities>(serde_json::to_value(&pools).unwrap())
            .unwrap(),
        pools
    );
}

#[test]
fn socket_and_resource_shapes_are_strict_and_toml_representable() {
    for value in [
        json!({"kind":"none","slot":0}),
        json!({"kind":"socket","slot":64}),
        json!(["socket", 0]),
    ] {
        assert!(serde_json::from_value::<PrimaryAttackSocket>(value).is_err());
    }
    for value in [
        json!({"kind":"none","units":1}),
        json!({"kind":"shared","resource":"grip","units":0}),
        json!({"kind":"shared","resource":"grip","units":65}),
    ] {
        assert!(serde_json::from_value::<EquipmentCapacityUse>(value).is_err());
    }
    for value in [
        PrimaryAttackSocket::None,
        PrimaryAttackSocket::Socket {
            slot: EquipmentSlotId(0),
        },
    ] {
        let encoded = toml::to_string(&value).unwrap();
        assert_eq!(
            toml::from_str::<PrimaryAttackSocket>(&encoded).unwrap(),
            value
        );
    }
    for value in [
        EquipmentCapacityUse::None,
        EquipmentCapacityUse::Shared {
            resource: id("grip"),
            units: 1,
        },
    ] {
        let encoded = toml::to_string(&value).unwrap();
        assert_eq!(
            toml::from_str::<EquipmentCapacityUse>(&encoded).unwrap(),
            value
        );
    }
}

#[test]
fn anatomy_requires_explicit_known_resources_and_attack_capable_primary() {
    let rules = crate::test_creatures::ruleset();
    let mut anatomy = crate::AnatomySpec::from_rules(&rules);
    assert!(anatomy.valid(&rules));
    anatomy.capacities = EquipmentCapacities::default();
    assert!(!anatomy.valid(&rules));
    anatomy.capacities = capacities(1);
    assert!(anatomy.valid(&rules));
    anatomy.capacities = capacities(3);
    assert!(anatomy.valid(&rules));
    anatomy.primary_attack_socket = PrimaryAttackSocket::Socket {
        slot: EquipmentSlotId(1),
    };
    assert!(!anatomy.valid(&rules));
    anatomy.primary_attack_socket = PrimaryAttackSocket::None;
    assert!(anatomy.valid(&rules));
    assert!(crate::AnatomySpec::default().valid(&rules));
}

#[test]
fn anatomy_records_require_all_fields_and_bounded_named_slots() {
    let rules = crate::test_creatures::ruleset();
    let original = serde_json::to_value(crate::AnatomySpec::from_rules(&rules)).unwrap();
    for field in ["slots", "capacities", "primary_attack_socket"] {
        let mut missing = original.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<crate::AnatomySpec>(missing).is_err());
    }
    let mut invalid = original.clone();
    invalid["slots"] = json!(vec!["weapon"; 65]);
    assert!(serde_json::from_value::<crate::AnatomySpec>(invalid).is_err());
    let mut invalid = original;
    invalid["primary_attack_socket"] = json!({"kind":"socket","slot":63});
    assert!(serde_json::from_value::<crate::AnatomySpec>(invalid).is_err());
    assert!(serde_json::from_value::<crate::AnatomySpec>(json!([[],{}, {"kind":"none"}])).is_err());
    let anatomy = crate::AnatomySpec::default();
    let encoded = toml::to_string(&anatomy).unwrap();
    assert_eq!(
        toml::from_str::<crate::AnatomySpec>(&encoded).unwrap(),
        anatomy
    );
}
