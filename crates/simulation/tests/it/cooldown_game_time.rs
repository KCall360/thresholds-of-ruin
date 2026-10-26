//! Cooldown fixtures enter through current save records. These tests exercise
//! Game clocks and region storage, not ordinary ability preparation ownership.
use super::creature_fixture as fixture;
use serde_json::{json, Value};
use std::{borrow::Cow, collections::BTreeMap, collections::BTreeSet, num::NonZeroU64, sync::Arc};
use tor_simulation::{
    checkpoint::{SharedState, Snapshot},
    content_ids::{AbilityId, ConditionId, TalentId},
    creatures::{record::BuildRecord, Content, CreatureBuild},
    diagnostics::work_counts,
    progression::HdSource,
    rules::{Modifier, ModifierOp, Ruleset, Scope, Stat},
    Action, ActorId, CreatureIdentity, Game, RecordId, RecordStore, RegionRecord, RegionRecordData,
    RegionState, RegionTransition, TransitionError,
};
use tor_world::{Extent, Location, Position, Region, RegionId, Shared, World};

const DRIVER: ActorId = ActorId(1);
const SUBJECT: ActorId = ActorId(2);

fn ability(id: &str) -> AbilityId {
    AbilityId::new(id).unwrap()
}

fn game(build: CreatureBuild, turns: [u64; 2]) -> Game {
    let world = World::new(
        [1, 3]
            .into_iter()
            .map(|id| Region {
                id: RegionId(id),
                name: format!("cooldown room {id}"),
                bounds: Extent::new(8, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42, build.content().clone());
    for (region, turns) in [1, 3].into_iter().zip(turns) {
        let id = game
            .spawn_actor(
                Location {
                    region: RegionId(region),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(turns).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            id,
            CreatureIdentity {
                name: format!("cooldown subject {region}"),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.configure_run(DRIVER, BTreeSet::from([DRIVER]), None, Default::default())
        .unwrap();
    game.add_default_reference_points().unwrap();
    game.refresh_navigation();
    game
}

fn checkpoint(game: &Game) -> Value {
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    serde_json::to_value((snapshot, shared)).unwrap()
}

fn restore(bytes: &[u8], original: &Game) -> Result<Game, String> {
    let (snapshot, shared): (Snapshot, SharedState) =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    Game::restore_checkpoint(snapshot, &shared, original.content().clone())
        .map_err(|error| error.to_string())
}

fn round_trip(game: &Game) -> Game {
    restore(&serde_json::to_vec(&checkpoint(game)).unwrap(), game).unwrap()
}

fn actor(value: &Value, id: ActorId) -> &Value {
    let table = value[0]["actors"].as_u64().unwrap() as usize;
    &value[1]["actors"][table][id.0.to_string()]
}

fn actor_mut(value: &mut Value, id: ActorId) -> &mut Value {
    let table = value[0]["actors"].as_u64().unwrap() as usize;
    &mut value[1]["actors"][table][id.0.to_string()]
}

fn seed(game: &Game, id: ActorId, rows: Value) -> Game {
    // Fixture setup only: use the strict public restore path rather than adding
    // a production Game API that bypasses future preparation authority.
    let mut value = checkpoint(game);
    actor_mut(&mut value, id)["combat"]["creature"]["cooldowns"] = rows;
    restore(&serde_json::to_vec(&value).unwrap(), game).unwrap()
}

fn remaining(game: &Game, id: ActorId, name: &str) -> u64 {
    game.creature(id)
        .unwrap()
        .cooldowns()
        .remaining(&ability(name))
}

fn advance_to(game: &mut Game, until: u64) {
    while game.tick() < until {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(
        game.tick(),
        until,
        "fixture must reach the requested boundary"
    );
}

fn transition(active: &[u64], loaded: &[u64]) -> RegionTransition {
    RegionTransition {
        active: active.iter().copied().map(RegionId).collect(),
        loaded: loaded.iter().copied().map(RegionId).collect(),
    }
}

/// A test-only store that always decodes detached records from bytes, including
/// during attachment. No surviving in-memory RegionRecord can mask a bad restore.
#[derive(Clone)]
struct EncodedRecords {
    bytes: BTreeMap<RecordId, Vec<u8>>,
    content: Arc<Content>,
}

impl EncodedRecords {
    fn new(game: &Game) -> Self {
        Self {
            bytes: BTreeMap::new(),
            content: game.content().clone(),
        }
    }

    fn round_trip(&self) -> Self {
        Self {
            bytes: serde_json::from_slice(&serde_json::to_vec(&self.bytes).unwrap()).unwrap(),
            content: self.content.clone(),
        }
    }

    fn decode(&self, bytes: &[u8]) -> Result<RegionRecord, String> {
        serde_json::from_slice::<RegionRecordData>(bytes)
            .map_err(|error| error.to_string())?
            .restore(self.content.clone())
            .map_err(|error| error.to_string())
    }
}

impl RecordStore for EncodedRecords {
    fn put(&mut self, id: RecordId, record: Shared<RegionRecord>) {
        assert!(self
            .bytes
            .insert(id, serde_json::to_vec(&*record).unwrap())
            .is_none());
    }

    fn get(&mut self, id: RecordId) -> Option<Shared<RegionRecord>> {
        self.decode(self.bytes.get(&id)?).ok().map(Shared::new)
    }
}

#[test]
fn target_projection_is_read_only_and_mutation_settles_only_earned_active_time() {
    let base = game(fixture::build(fixture::species()), [99, 150]);
    let mut game = seed(
        &base,
        SUBJECT,
        json!([{"ability": "spark_lance_fire", "remaining": 500}]),
    );
    game = seed(
        &game,
        DRIVER,
        json!([{"ability": "basic_melee", "remaining": 900}]),
    );
    advance_to(&mut game, 99);
    let before = checkpoint(&game);
    assert_eq!(actor(&before, SUBJECT)["active_since"], json!(0));
    assert_eq!(
        actor(&before, SUBJECT)["combat"]["creature"]["cooldowns"][0]["remaining"],
        json!(500)
    );
    let untouched = game.clone();
    let counts = work_counts();
    let projected = game.creature(SUBJECT).unwrap();
    assert!(matches!(&projected, Cow::Owned(_)));
    assert_eq!(
        projected
            .cooldowns()
            .remaining(&ability("spark_lance_fire")),
        401
    );
    assert_eq!(
        work_counts().cooldown_entries_advanced - counts.cooldown_entries_advanced,
        1
    );
    drop(projected);
    assert_eq!(game, untouched);
    assert_eq!(checkpoint(&game), before);
    assert_eq!(game.next_creature_event_tick(), Some(500));
    assert_eq!(round_trip(&game), game);

    // A rejected mutation must not accidentally publish its settled candidate.
    assert!(game
        .apply_condition(SUBJECT, &ConditionId::new("missing").unwrap(), DRIVER, 20)
        .is_err());
    assert_eq!(checkpoint(&game), before);
    game.apply_condition(SUBJECT, &ConditionId::new("fear").unwrap(), DRIVER, 20)
        .unwrap();
    let settled = checkpoint(&game);
    assert_eq!(actor(&settled, SUBJECT)["active_since"], json!(99));
    assert_eq!(
        actor(&settled, SUBJECT)["combat"]["creature"]["cooldowns"][0]["remaining"],
        json!(401)
    );
    assert_eq!(actor(&settled, DRIVER), actor(&before, DRIVER));
    assert_eq!(game.next_creature_event_tick(), Some(119));
    let mut restored = round_trip(&game);
    advance_to(&mut game, 150);
    advance_to(&mut restored, 150);
    assert_eq!(remaining(&game, SUBJECT, "spark_lance_fire"), 350);
    assert_eq!(game, restored);
}

#[test]
fn cooldowns_follow_fast_forward_resting_and_moving_physics_equally() {
    let mut expected = None;
    for mode in 0..3 {
        let base = game(fixture::build(fixture::species()), [99, 150]);
        let mut game = seed(
            &base,
            SUBJECT,
            json!([{"ability": "spark_lance_fire", "remaining": 250}]),
        );
        if mode == 1 {
            game.set_gravity(RegionId(3), [0, 0, -1]).unwrap();
        } else if mode == 2 {
            game.set_actor_velocity(SUBJECT, [1, 0, 0]).unwrap();
        }
        let counts = work_counts();
        advance_to(&mut game, 99);
        let steps = work_counts().physics_steps - counts.physics_steps;
        assert_eq!(steps > 0, mode == 2, "exercise the selected physics path");
        assert_eq!(remaining(&game, SUBJECT, "spark_lance_fire"), 151);
        assert_eq!(game.next_creature_event_tick(), Some(250));
        let state = game.creature(SUBJECT).unwrap().into_owned();
        if let Some(expected) = &expected {
            assert_eq!(&state, expected);
        } else {
            expected = Some(state);
        }
        let mut restored = round_trip(&game);
        advance_to(&mut game, 297);
        advance_to(&mut restored, 297);
        assert_eq!(game, restored);
        assert_eq!(remaining(&game, SUBJECT, "spark_lance_fire"), 0);
        assert_eq!(game.next_creature_event_tick(), None);
    }
}

#[test]
fn deadlines_expire_exactly_and_prune_only_the_due_cooldown_rows() {
    let base = game(fixture::build(fixture::species()), [1, 1]);
    let mut game = seed(
        &base,
        SUBJECT,
        json!([
            {"ability": "spark_lance_fire", "remaining": 23},
            {"ability": "basic_melee", "remaining": 17}
        ]),
    );
    game = seed(
        &game,
        DRIVER,
        json!([{"ability": "spark_lance_fire", "remaining": 31}]),
    );
    advance_to(&mut game, 16);
    assert_eq!(remaining(&game, SUBJECT, "basic_melee"), 1);
    assert_eq!(game.next_creature_event_tick(), Some(17));
    game = round_trip(&game);
    advance_to(&mut game, 17);
    assert_eq!(game.next_creature_event_tick(), Some(23));
    assert_eq!(
        actor(&checkpoint(&game), SUBJECT)["combat"]["creature"]["cooldowns"],
        json!([{"ability": "spark_lance_fire", "remaining": 6}])
    );
    advance_to(&mut game, 22);
    assert_eq!(remaining(&game, SUBJECT, "spark_lance_fire"), 1);
    advance_to(&mut game, 23);
    assert_eq!(game.next_creature_event_tick(), Some(31));
    assert_eq!(
        actor(&checkpoint(&game), SUBJECT)["combat"]["creature"]["cooldowns"],
        json!([])
    );
    advance_to(&mut game, 31);
    assert_eq!(game.next_creature_event_tick(), None);
    let counts = work_counts();
    advance_to(&mut game, 40);
    assert_eq!(
        work_counts().cooldown_entries_advanced - counts.cooldown_entries_advanced,
        0
    );
    assert_eq!(round_trip(&game), game);
}

#[test]
fn frozen_and_detached_checkpoints_do_not_credit_catch_up_time() {
    for detach in [false, true] {
        let base = game(fixture::build(fixture::species()), [1, 1]);
        let mut game = seed(
            &base,
            SUBJECT,
            json!([
                {"ability": "basic_melee", "remaining": 17},
                {"ability": "spark_lance_fire", "remaining": 30}
            ]),
        );
        advance_to(&mut game, 7);
        let before = game.creature(SUBJECT).unwrap().into_owned();
        let mut records = EncodedRecords::new(&game);
        let frozen = transition(&[1], if detach { &[1] } else { &[1, 3] });
        game.transition_regions(&frozen, &mut records).unwrap();
        assert_eq!(
            game.region_state(RegionId(3)),
            Some(if detach {
                RegionState::Detached
            } else {
                RegionState::Frozen
            })
        );
        assert_eq!(game.next_creature_event_tick(), None);
        let counts = work_counts();
        advance_to(&mut game, 107);
        assert_eq!(
            work_counts().cooldown_entries_advanced - counts.cooldown_entries_advanced,
            0
        );
        if detach {
            assert!(game.creature(SUBJECT).is_none());
            assert_eq!(records.bytes.len(), 1);
        } else {
            assert_eq!(game.creature(SUBJECT).as_deref(), Some(&before));
        }
        game = round_trip(&game);
        records = records.round_trip();
        assert!(game.detached_records_valid(&mut records));
        assert_eq!(game.next_creature_event_tick(), None);
        // Reattachment while still frozen must not reinsert an expired absolute deadline.
        game.transition_regions(&transition(&[1], &[1, 3]), &mut records)
            .unwrap();
        assert_eq!(game.creature(SUBJECT).as_deref(), Some(&before));
        assert_eq!(game.next_creature_event_tick(), None);
        advance_to(&mut game, 207);
        game = round_trip(&game);
        game.transition_regions(&transition(&[1, 3], &[1, 3]), &mut records)
            .unwrap();
        assert_eq!(game.creature(SUBJECT).as_deref(), Some(&before));
        assert_eq!(game.next_creature_event_tick(), Some(217));
        let mut replay = round_trip(&game);
        for state in [&mut game, &mut replay] {
            advance_to(state, 216);
            assert_eq!(remaining(state, SUBJECT, "basic_melee"), 1);
            advance_to(state, 217);
            assert_eq!(remaining(state, SUBJECT, "basic_melee"), 0);
            assert_eq!(remaining(state, SUBJECT, "spark_lance_fire"), 13);
            assert_eq!(state.next_creature_event_tick(), Some(230));
            advance_to(state, 230);
            assert_eq!(state.next_creature_event_tick(), None);
        }
        assert_eq!(game, replay);
    }
}

#[test]
fn same_tick_freeze_and_thaw_restores_cooldown_deadline_membership() {
    let base = game(fixture::build(fixture::species()), [1, 1]);
    let mut game = seed(
        &base,
        SUBJECT,
        json!([{"ability": "spark_lance_fire", "remaining": 17}]),
    );
    let mut records = EncodedRecords::new(&game);
    game.transition_regions(&transition(&[1], &[1, 3]), &mut records)
        .unwrap();
    assert_eq!(game.next_creature_event_tick(), None);
    game = round_trip(&game);
    game.transition_regions(&transition(&[1, 3], &[1, 3]), &mut records)
        .unwrap();
    assert_eq!(game.tick(), 0);
    assert_eq!(game.next_creature_event_tick(), Some(17));
    advance_to(&mut game, 17);
    assert_eq!(remaining(&game, SUBJECT, "spark_lance_fire"), 0);
    assert_eq!(game.next_creature_event_tick(), None);
}

#[test]
fn rebuild_and_grant_loss_keep_the_settled_cooldown_and_original_deadline() {
    let original = fixture::build_with(
        fixture::species(),
        vec![],
        &[HdSource::Racial, HdSource::Racial],
    );
    let talent = TalentId::new("fixture_cooldown_grant").unwrap();
    let bolt = ability("spark_lance_fire");
    let mut document = original.ruleset().document().clone();
    document.talents.push(fixture::talent(
        "fixture_cooldown_grant",
        vec![fixture::add(Stat::Health, 2)],
    ));
    document
        .talents
        .iter_mut()
        .find(|entry| entry.id == talent)
        .unwrap()
        .modifiers
        .extend([
            Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: bolt.clone(),
                },
            },
            Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Attribute {
                        attribute: tor_simulation::attributes::Attribute::Speed,
                    },
                    value: 10,
                },
            },
        ]);
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        original.content().catalog().clone(),
    )
    .unwrap();
    let mut granted = BuildRecord::capture(&original).restore(content).unwrap();
    granted
        .select_talent(tor_simulation::HdIndex(1), talent)
        .unwrap();
    let base = game(granted.clone(), [99, 150]);
    let mut game = seed(
        &base,
        SUBJECT,
        json!([{"ability": "spark_lance_fire", "remaining": 250}]),
    );
    advance_to(&mut game, 99);
    assert!(matches!(game.creature(SUBJECT).unwrap(), Cow::Owned(_)));
    assert!(game
        .creature(SUBJECT)
        .unwrap()
        .derived()
        .abilities
        .contains(&bolt));
    let mut drained = granted.clone();
    drained.remove_latest().unwrap();
    game.rebuild_creature(SUBJECT, drained).unwrap();
    assert!(!game
        .creature(SUBJECT)
        .unwrap()
        .derived()
        .abilities
        .contains(&bolt));
    assert_eq!(remaining(&game, SUBJECT, "spark_lance_fire"), 151);
    assert_eq!(
        actor(&checkpoint(&game), SUBJECT)["active_since"],
        json!(99)
    );
    assert_eq!(game.next_creature_event_tick(), Some(250));
    game = round_trip(&game);
    advance_to(&mut game, 150);
    game.rebuild_creature(SUBJECT, granted).unwrap();
    assert!(game
        .creature(SUBJECT)
        .unwrap()
        .derived()
        .abilities
        .contains(&bolt));
    assert_eq!(remaining(&game, SUBJECT, "spark_lance_fire"), 100);
    assert_eq!(game.next_creature_event_tick(), Some(250));
    advance_to(&mut game, 297);
    assert_eq!(remaining(&game, SUBJECT, "spark_lance_fire"), 0);
    assert_eq!(game.next_creature_event_tick(), None);
}

#[test]
fn lethal_condition_expiry_stops_cooldowns_at_the_game_death_boundary() {
    let original = fixture::build(fixture::species());
    let fear = ConditionId::new("fear").unwrap();
    let mut document = original.ruleset().document().clone();
    document
        .conditions
        .iter_mut()
        .find(|entry| entry.id == fear)
        .unwrap()
        .modifiers
        .push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Health,
                value: 100,
            },
        });
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        original.content().catalog().clone(),
    )
    .unwrap();
    let build = BuildRecord::capture(&original).restore(content).unwrap();
    for mode in 0..3 {
        let base = game(build.clone(), [1, 1]);
        let mut game = seed(
            &base,
            SUBJECT,
            json!([
                {"ability": "basic_melee", "remaining": 10},
                {"ability": "spark_lance_fire", "remaining": 100}
            ]),
        );
        let baseline = game.health(SUBJECT).unwrap().0;
        game.apply_condition(SUBJECT, &fear, DRIVER, 10).unwrap();
        fixture::injure(&mut game, SUBJECT, u32::try_from(baseline + 5).unwrap()).unwrap();
        if mode == 1 {
            game.set_gravity(RegionId(3), [0, 0, -1]).unwrap();
        } else if mode == 2 {
            game.set_actor_velocity(SUBJECT, [1, 0, 0]).unwrap();
        }
        advance_to(&mut game, 9);
        assert!(game.alive(SUBJECT));
        assert_eq!(remaining(&game, SUBJECT, "spark_lance_fire"), 91);
        assert_eq!(game.next_creature_event_tick(), Some(10));
        game = round_trip(&game);
        advance_to(&mut game, 10);
        assert!(!game.alive(SUBJECT));
        assert!(!game.run_outcome().terminal);
        assert_eq!(remaining(&game, SUBJECT, "basic_melee"), 0);
        assert_eq!(remaining(&game, SUBJECT, "spark_lance_fire"), 90);
        assert_eq!(game.next_creature_event_tick(), None);
        let dead = game.creature(SUBJECT).unwrap().into_owned();
        advance_to(&mut game, 120);
        game.rebuild_creature(SUBJECT, build.clone()).unwrap();
        game = round_trip(&game);
        assert_eq!(game.creature(SUBJECT).as_deref(), Some(&dead));
        assert_eq!(game.next_creature_event_tick(), None);
    }
}

fn invalid_rows() -> Vec<Option<Value>> {
    vec![
        None,
        Some(Value::Null),
        Some(json!({"magic_bolt": 1})),
        Some(json!([["magic_bolt", 1]])),
        Some(json!([{"ability": "missing", "remaining": 1}])),
        Some(json!([{"ability": "bad id", "remaining": 1}])),
        Some(json!([{"ability": "spark_lance_fire", "remaining": 0}])),
        Some(json!([{"ability": "spark_lance_fire", "remaining": 1_000_001}])),
        Some(json!([{"ability": "spark_lance_fire", "remaining": u64::MAX}])),
        Some(json!([{"ability": "spark_lance_fire", "remaining": -1}])),
        Some(json!([{"ability": "spark_lance_fire", "remaining": 1.5}])),
        Some(json!([{"ability": "spark_lance_fire"}])),
        Some(json!([{"remaining": 1}])),
        Some(json!([{"ability": "spark_lance_fire", "remaining": 1, "elapsed": 0}])),
        Some(json!([
            {"ability": "spark_lance_fire", "remaining": 1},
            {"ability": "spark_lance_fire", "remaining": 2}
        ])),
        Some(Value::Array(vec![
            json!({"ability": "spark_lance_fire", "remaining": 1});
            tor_simulation::limits::CATALOG_ENTRIES
                + 1
        ])),
    ]
}

fn corrupt_rows(creature: &mut Value, rows: Option<Value>) {
    if let Some(rows) = rows {
        creature["cooldowns"] = rows;
    } else {
        creature.as_object_mut().unwrap().remove("cooldowns");
    }
}

fn duplicate_fields(bytes: &[u8]) -> Vec<Vec<u8>> {
    let encoded = std::str::from_utf8(bytes).unwrap();
    [
        ("\"remaining\":17", "\"remaining\":17,\"remaining\":2"),
        (
            "\"ability\":\"spark_lance_fire\"",
            "\"ability\":\"spark_lance_fire\",\"ability\":\"basic_melee\"",
        ),
        ("\"cooldowns\":[{", "\"cooldowns\":[],\"cooldowns\":[{"),
    ]
    .into_iter()
    .map(|(from, to)| {
        let duplicate = encoded.replace(from, to);
        assert_ne!(duplicate, encoded, "exercise duplicate {from}");
        duplicate.into_bytes()
    })
    .collect()
}

#[test]
fn current_game_checkpoints_reject_malformed_cooldown_records() {
    let base = game(fixture::build(fixture::species()), [1, 1]);
    let game = seed(
        &base,
        SUBJECT,
        json!([{"ability": "spark_lance_fire", "remaining": 17}]),
    );
    let valid = checkpoint(&game);
    assert_eq!(round_trip(&game), game, "positive control");
    for rows in invalid_rows() {
        let mut invalid = valid.clone();
        corrupt_rows(
            &mut actor_mut(&mut invalid, SUBJECT)["combat"]["creature"],
            rows.clone(),
        );
        assert!(
            restore(&serde_json::to_vec(&invalid).unwrap(), &game).is_err(),
            "accepted {rows:?}"
        );
    }
    for bytes in duplicate_fields(&serde_json::to_vec(&valid).unwrap()) {
        assert!(restore(&bytes, &game).is_err());
    }
    assert_eq!(checkpoint(&game), valid);
}

#[test]
fn malformed_detached_cooldowns_fail_decode_and_attachment_without_publication() {
    let base = game(fixture::build(fixture::species()), [1, 1]);
    let mut game = seed(
        &base,
        SUBJECT,
        json!([{"ability": "spark_lance_fire", "remaining": 17}]),
    );
    let mut records = EncodedRecords::new(&game);
    game.transition_regions(&transition(&[1], &[1]), &mut records)
        .unwrap();
    advance_to(&mut game, 30);
    game = round_trip(&game);
    let before = checkpoint(&game);
    let id = *records.bytes.keys().next().unwrap();
    let bytes = records.bytes[&id].clone();
    let valid: Value = serde_json::from_slice(&bytes).unwrap();
    let attach = transition(&[1, 3], &[1, 3]);
    let mut control = game.clone();
    control
        .transition_regions(&attach, &mut records.round_trip())
        .unwrap();
    assert_eq!(
        remaining(&control, SUBJECT, "spark_lance_fire"),
        17,
        "positive control"
    );
    let mut corruptions = duplicate_fields(&bytes);
    for rows in invalid_rows() {
        let mut invalid = valid.clone();
        corrupt_rows(
            &mut invalid["actors"][SUBJECT.0.to_string()]["combat"]["creature"],
            rows,
        );
        corruptions.push(serde_json::to_vec(&invalid).unwrap());
    }
    for bytes in corruptions {
        assert!(records.decode(&bytes).is_err());
        let mut invalid = records.round_trip();
        invalid.bytes.insert(id, bytes);
        assert_eq!(
            game.transition_regions(&attach, &mut invalid),
            Err(TransitionError::RecordUnavailable(RegionId(3)))
        );
        assert_eq!(checkpoint(&game), before);
        assert!(!game.has_actor(SUBJECT));
        assert_eq!(game.next_creature_event_tick(), None);
    }
}

#[test]
fn terminal_physics_credits_cooldowns_only_to_the_actual_death_tick() {
    let base = game(fixture::build(fixture::species()), [99, 150]);
    let mut game = seed(
        &base,
        SUBJECT,
        json!([{"ability": "spark_lance_fire", "remaining": 100}]),
    );
    game = seed(
        &game,
        DRIVER,
        json!([{"ability": "spark_lance_fire", "remaining": 100}]),
    );
    game.teleport(
        DRIVER,
        Location {
            region: RegionId(1),
            position: Position { x: 7, y: 1, z: 0 },
        },
    )
    .unwrap();
    let injury = game.health(DRIVER).unwrap().0 - 1;
    fixture::injure(&mut game, DRIVER, u32::try_from(injury).unwrap()).unwrap();
    game.set_actor_velocity(DRIVER, [8192, 0, 0]).unwrap();
    game.act(DRIVER, Action::Wait).unwrap();
    game.act(SUBJECT, Action::Wait).unwrap();
    assert!((1..99).contains(&game.tick()));
    assert!(!game.alive(DRIVER));
    assert!(game.run_outcome().terminal);
    let expected = 100 - game.tick();
    assert_eq!(remaining(&game, DRIVER, "spark_lance_fire"), expected);
    assert_eq!(remaining(&game, SUBJECT, "spark_lance_fire"), expected);
    assert_eq!(game.next_creature_event_tick(), Some(100));
    assert_eq!(round_trip(&game), game);
}
