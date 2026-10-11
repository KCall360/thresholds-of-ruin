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
    }
}
pub fn stats() -> OwnStats {
    OwnStats {
        kind: content("humanoid"),
        subtypes: vec![],
        hit_dice: vec![content("racial")],
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
        resources: vec![],
        active_talents: vec![],
        dormant_talents: vec![],
        abilities: vec![ability("basic_melee")],
    }
}
pub fn observation() -> Observation {
    Observation {
        interactions: None,
        combat: Some(CombatView {
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
        visible_actors: vec![ActorView {
            asset: None,
            id: ActorTarget::from_digest([2; 32]),
            name: "sentinel".into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
        }],
        ready: true,
    }
}
