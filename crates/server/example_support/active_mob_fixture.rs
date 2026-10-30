//! Immutable active-mob workload v1. Ordinary authored content, never a game mode.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tor_server::creature_authoring::{Modifier, Op, Scope, Stat};
use tor_server::scenario_package::{self, AiProfile, Portal};
use tor_server::{Scenario, Streaming};

pub const REGIONS: u64 = 20;
pub const MOBS_PER_REGION: u64 = 5;
pub const MOBS: u64 = REGIONS * MOBS_PER_REGION;
pub const SEED: u64 = 42;
pub const WIDTH: i32 = 8;

pub fn region_of(actor: u64) -> Option<u64> {
    (2..=MOBS + 1)
        .contains(&actor)
        .then(|| (actor - 2) / MOBS_PER_REGION + 1)
}

pub fn make(root: &Path) -> Result<Scenario, Box<dyn std::error::Error>> {
    let template = scenario_package::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/dungeon-loop"),
        SEED,
        None,
        false,
    )?;
    let mut package = (**template.package.as_ref().ok_or("missing package")?).clone();
    package.manifest.id = "active-mobs-v1".into();
    package.manifest.version = "1.0".into();
    package.manifest.objective = None;
    package.manifest.factions = BTreeMap::from([
        ("red".into(), BTreeSet::from(["blue".into()])),
        ("blue".into(), BTreeSet::from(["red".into()])),
        ("neutral".into(), BTreeSet::new()),
    ]);
    package.manifest.ai_profiles = BTreeMap::from([(
        "tactical".into(),
        AiProfile {
            idle: Some(tor_server::creature_authoring::IdleBehavior::Explore),
            doors: Some(true),
            care: Some(false),
            equipment: Some(false),
            loot: Some(false),
            gold: Some(false),
            flee_percent: Some(0),
        },
    )]);
    for species in package.manifest.creatures.species.values_mut() {
        species.modifiers.retain(|modifier| {
            !matches!(
                modifier.op,
                Op::Add {
                    stat: Stat::Health,
                    ..
                }
            )
        });
        species.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: Op::Add {
                stat: Stat::Health,
                value: 1_000_000 - 8,
            },
        });
    }
    let player = &mut package.manifest.characters[0];
    player.anchor = "1/start".into();
    player
        .creature
        .as_mut()
        .ok_or("missing player creature")?
        .faction = "neutral".into();
    let original = package.region_defs()?.remove(0);
    let enemy = original
        .actors
        .first()
        .ok_or("missing AI template")?
        .clone();
    let positions = [[1, 3, 0], [2, 3, 0], [6, 3, 0], [6, 6, 0], [1, 6, 0]];
    let mut regions = Vec::new();
    for id in 1..=REGIONS {
        let mut region = original.clone();
        region.id = id;
        region.name = format!("Active encounter {id}");
        region.size = [WIDTH, 9, 2];
        region.chamber = false;
        region.items.clear();
        region.anchors = BTreeMap::from([
            ("start".into(), [1, 0, 0]),
            ("west".into(), [0, 0, 0]),
            ("east".into(), [WIDTH - 1, 0, 0]),
        ]);
        // The ordinary player's corridor is not an artificially empty AI world:
        // five competing AI run in each adjoining chamber. Its full-height wall
        // prevents crowding from making movement throughput an obstruction test.
        region.walls = (0..WIDTH)
            .flat_map(|x| (0..2).map(move |z| [x, 1, z]))
            .collect();
        // A central full-height pillar gives pursuit a real navigation obstacle.
        region
            .walls
            .extend([[4, 4, 0], [4, 4, 1], [4, 5, 0], [4, 5, 1]]);
        region.portals = [
            ([WIDTH - 1, 0, 0], "east", id % REGIONS + 1, "west"),
            ([0, 0, 0], "west", (id + REGIONS - 2) % REGIONS + 1, "east"),
        ]
        .into_iter()
        .map(|(at, direction, to, anchor)| Portal {
            rotation: None,
            kind: None,
            at,
            direction: direction.into(),
            to: format!("{to}/{anchor}"),
            turns: 0,
            width: 1,
            height: 2,
        })
        .collect();
        region.actors = positions
            .into_iter()
            .enumerate()
            .map(|(index, at)| {
                let mut actor = enemy.clone();
                actor.id = 2 + (id - 1) * MOBS_PER_REGION + index as u64;
                actor.at = at;
                actor.turn_ticks = Some(100);
                let creature = actor.creature.as_mut().expect("template creature");
                creature.name = format!("encounter {id} mob {}", index + 1);
                creature.faction = if (index as u64 + id).is_multiple_of(2) {
                    "red"
                } else {
                    "blue"
                }
                .into();
                actor
            })
            .collect();
        regions.push(region);
    }
    scenario_package::write_package(root, &package.manifest, &regions, &package.rules_source)?;
    scenario_package::validate(root)?;
    let mut scenario = scenario_package::load(root, SEED, None, false)?;
    // This is deliberately an all-active stress case, not a large catalog with
    // frozen AI. Keep the ordinary streaming/transition implementation enabled.
    scenario.streaming = Some(Streaming {
        active_radius: REGIONS as u32,
        load_radius: REGIONS as u32,
    });
    Ok(scenario)
}
