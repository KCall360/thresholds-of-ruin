//! Disclosed fixture content for client tests, with arbitrary IDs supported.
use tor_protocol::*;
pub fn content(id: &str) -> ContentRef {
    ContentRef {
        id: id.into(),
        name: id
            .split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}
pub fn equipment_slot(id: &str, equipped_phrase: &str, requires_attack: bool) -> EquipmentSlot {
    EquipmentSlot {
        id: id.into(),
        name: content(id).name,
        equipped_phrase: equipped_phrase.into(),
        requires_attack,
    }
}
pub fn ability(id: &str) -> AbilityView {
    AbilityView {
        id: id.into(),
        name: content(id).name,
        cost: None,
        reach: if id == "basic_melee" {
            AbilityReach::Melee
        } else {
            AbilityReach::Sight { range: 6 }
        },
        target_kind: AbilityTargetKind::Actor,
        basic_attack: id == "basic_melee",
        weapon_selection: id == "basic_melee",
        cooldown_remaining: 0,
        supply_costs: vec![],
    }
}
pub fn stats() -> OwnStats {
    OwnStats {
        kind: content("humanoid"),
        subtypes: vec![],
        hit_dice: HitDiceSummary {
            total: 1,
            sources: vec![HitDiceSourceCount {
                source_kind: HitDieSourceKind::Racial,
                source: content("racial"),
                count: 1,
            }],
        },
        attributes: AttributeView {
            strength: 1,
            speed: 1,
            intellect: 1,
            willpower: 1,
            awareness: 1,
            presence: 1,
        },
        skills: vec![],
        defenses: DefenseView {
            physical: 12,
            cognitive: 12,
            spiritual: 12,
        },
        binding: content("intellect"),
        nutrition: tor_protocol::NutritionView::Exempt,
        restorable_losses: RestorableLossesView {
            hit_dice: 0,
            attributes: AttributeView {
                strength: 0,
                speed: 0,
                intellect: 0,
                willpower: 0,
                awareness: 0,
                presence: 0,
            },
            maximum_health: 0,
        },
        conditions: vec![],
        health_recovery: tor_protocol::HealthRecoveryView {
            period_ticks: 1200,
            elapsed_ticks: 0,
            stopped: false,
            reset_percent: 100,
        },
        resources: vec![],
        expertises: vec![],
        active_talents: vec![],
        dormant_talents: vec![],
        abilities: vec![ability("basic_melee")],
    }
}
pub fn observation() -> Observation {
    Observation {
        self_appearance: None,
        interactions: None,
        combat: Some(CombatView {
            public_plan: None,
            sequence: None,
            own_stats: Some(stats()),
            hp: 12,
            max_hp: 12,
            preparation_remaining: None,
            preparation_active: false,
            recovery_remaining: 0,
            actors: vec![],
            events: vec![],
            objective: None,
            exit: None,
            victory: false,
            dead: false,
            terminal: false,
        }),
        motion: None,
        places: vec![],
        actor: ActorId(1),
        self_target: ActorTarget::from_digest([1; 32]),
        tick: 0,
        position: Position { x: 0, y: 0, z: 0 },
        visible_cells: vec![],
        ground_items: vec![],
        inventory: vec![],
        charts: Vec::new(),
        linked_holds: vec![],
        visible_actors: vec![ActorView {
            appearance_seed: None,
            worn: Vec::new(),
            asset: None,
            id: ActorTarget::from_digest([2; 32]),
            name: "sentinel".into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
        }],
        ready: true,
    }
}

/// Valid carried-tool disclosure for actual caller tests, independent of identity.
pub fn portable_light_observation(lit: bool) -> Observation {
    let mut view = observation();
    let item = ItemTarget::from_digest([83; 32]);
    let slot = equipment_slot("held_light", "held in one hand", false);
    view.inventory = vec![ItemView {
        id: item,
        name: "torch".into(),
        class: ItemClass::Tool,
        appearance: "torch".into(),
        identified: true,
        quantity: 1,
        description: String::new(),
        corpse: None,
        asset: None,
        known_currency: None,
        known_supply: None,
    }];
    view.interactions = Some(InteractionView {
        slots: vec![slot.clone()],
        preparation: None,
        completed: vec![],
        refused: vec![],
        curse_blocked: vec![],
        identification: None,
        inventory: vec![ItemInteractionView {
            item,
            slot: Some(slot),
            equipped_slot: Some(0),
            known_equipment: None,
            known_projectile_bonus: None,
            known_cursed: None,
            known_protected: None,
            drinkable: false,
            edible: false,
            readable: false,
            zappable: false,
            device_activation: None,
            charges: None,
            portable_light: Some(PortableLightView {
                remaining: 100_000,
                lit,
                ignitable: !lit,
                extinguishable: lit,
            }),
        }],
    });
    StateView {
        snapshot: tor_protocol::SnapshotId("fixture".into()),
        revision: 0,
        wizard_game: false,
        observation: view.clone(),
    }
    .validate()
    .expect("valid physical portable-light disclosure");
    view
}
