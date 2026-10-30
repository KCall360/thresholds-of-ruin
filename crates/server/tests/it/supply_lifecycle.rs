//! Finite supply authority through ordinary authored packages and the Engine.
use crate::support;
use serde_json::{json, Value};
use std::{io::Write, path::Path, sync::Arc};
use tor_protocol::ActorId;
use tor_server::{
    journal::{Action, Command, IntentionEndKind, JournalContent, SelectedSupply, SupplySelection},
    scenario_package::{Manifest, Package, RegionDef},
    CommandResult, Engine, SavePolicy, Scenario,
};

const PLAYER: ActorId = ActorId(1);
const PARTNER: ActorId = ActorId(2);

fn damage(amount: u32) -> Value {
    json!({"kind":"damage","damage":{"kind":"components","components":[{
        "category":"vital","amount":{"kind":"fixed","amount":amount}}]}})
}
fn strike(amount: u32) -> Value {
    json!({"offset":0,"on_success":[damage(amount)],"on_failure":[],"descriptors":[],"riders":[]})
}

/// Neither the canonical care definition nor natural recovery is overridden.
/// Injuries are authored consumables/attacks, then executed through ordinary actions.
fn scenario(hd: usize, bandages: u64, partner: Option<bool>, injury: u32) -> Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut manifest: Value = serde_json::to_value(
        toml::from_str::<Manifest>(&std::fs::read_to_string(root.join("scenario.toml")).unwrap())
            .unwrap(),
    )
    .unwrap();
    manifest["factions"] = json!({"carers":[],"foe":["carers"]});
    manifest["ai_profiles"] =
        json!({"pressure":{"care": true, "equipment": true, "loot": true, "flee_percent":0}});
    manifest["archetypes"]["bandage"] =
        json!({"name":"clean bandage","identity":"bandage", "stackable":true,"supply":"bandage"});
    manifest["archetypes"]["injury"] = json!({"name":"fixture injury dose","class":"potion","consumable":{"effects":[damage(injury)]}});
    manifest["creatures"] = json!({"species":{"carer":{
        "kind":"humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000},"attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":100,"recovery":100,"strikes":[strike(1)]}
    }}});
    let creature = |name: &str, faction: &str| {
        json!({"species":"carer","name":name,"faction":faction,
        "binding":"intellect","starting_skills":["heavy_weaponry","athletics"],"restoration":"authored", "nutrition": {"kind":"exempt"},
        "hit_dice":vec![json!({"source":"racial"});hd]})
    };
    manifest["characters"][0]["creature"] = creature("first carer", "carers");
    manifest["characters"][0]["turn_ticks"] = json!(100);
    let mut regions: Vec<RegionDef> = [1, 2]
        .into_iter()
        .map(|id| {
            toml::from_str(
                &std::fs::read_to_string(root.join(format!("regions/{id}.toml"))).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    let mut items =
        vec![json!({"id":11,"at":[1,1,0],"carried_by":1,"archetype":"injury","quantity":1})];
    if bandages > 0 {
        items.push(json!({"id":10,"at":[2,1,0],"archetype":"bandage","quantity":bandages}));
    }
    region["items"] = json!(items);
    region["actors"] = match partner {
        None => json!([]),
        Some(ai) => {
            let mut actor = json!({"id":2,"at":[if ai {2} else {3},1,0],"turn_ticks":100,
                "controller":if ai {"ai"} else {"external"},
                "creature":creature("second carer",if ai {"foe"} else {"carers"})});
            if ai {
                actor["ai"] = json!("pressure");
            }
            json!([actor])
        }
    };
    regions[0] = serde_json::from_value(region).unwrap();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        regions,
        tor_simulation::rules::DEFAULT_RULESET_TOML,
    )
    .unwrap();
    Scenario {
        actors: vec![],
        package: Some(Arc::new(package)),
        ..Scenario::two_room(42)
    }
}

fn care(actor: ActorId) -> Action {
    Action::UseAbility {
        id: tor_simulation::content_ids::AbilityId::new("bind_wounds").unwrap(),
        invocation: tor_server::journal::AbilityInvocation {
            aim: tor_server::journal::AbilityAim::ActorTarget {
                target: tor_simulation::ActorId(actor.0),
            },
            weapon: tor_server::journal::WeaponSelection::Default,
        },
        supplies: SupplySelection::Explicit {
            items: vec![SelectedSupply {
                cost_ordinal: 0,
                item: 10,
            }],
        },
    }
}
fn submit(engine: &mut Engine, actor: ActorId, id: &str, command: Command) -> CommandResult {
    engine
        .command(
            "player",
            "supply-test",
            actor,
            id,
            &engine.branch().clone(),
            command,
        )
        .unwrap()
}
fn admit(engine: &mut Engine, actor: ActorId, id: &str) -> (CommandResult, Command) {
    let command = Command::AdmitIntention {
        expected_boundary: engine.causal_boundary(actor).unwrap(),
        action: care(actor),
    };
    (submit(engine, actor, id, command.clone()), command)
}
fn combat(engine: &Engine, actor: ActorId) -> tor_protocol::CombatView {
    engine.state(actor).unwrap().observation.combat.unwrap()
}
fn cooldown(engine: &Engine, actor: ActorId) -> u64 {
    let view = engine.state(actor).unwrap().observation;
    view.combat
        .unwrap()
        .own_stats
        .unwrap()
        .abilities
        .iter()
        .find(|ability| ability.id == "bind_wounds")
        .unwrap()
        .cooldown_remaining
}
fn bandages(engine: &Engine, actor: ActorId) -> u64 {
    let view = engine.state(actor).unwrap().observation;
    view.inventory
        .iter()
        .chain(view.ground_items.iter().map(|ground| &ground.item))
        .filter(|item| {
            item.known_supply
                .as_ref()
                .is_some_and(|supply| supply.id == "bandage")
        })
        .map(|item| item.quantity)
        .sum()
}
fn step(engine: &mut Engine) {
    let actor = engine.next_actor().expect("live ordinary actor");
    if engine.is_ai(actor) {
        engine.advance_ai(actor).unwrap();
    } else {
        support::act_as(engine, actor, Action::Wait).unwrap();
    }
}
fn finish(engine: &mut Engine, actor: ActorId) {
    for _ in 0..100 {
        if combat(engine, actor).preparation_remaining.is_none() {
            return;
        }
        step(engine);
    }
    panic!("ordinary preparation did not finish");
}

#[test]
fn two_claimants_have_one_winner_without_exposing_held_quantity() {
    let mut engine = Engine::memory(scenario(1, 1, Some(false), 1)).unwrap();
    let public = engine.state(PARTNER).unwrap().observation.ground_items;
    admit(&mut engine, PLAYER, "first");
    admit(&mut engine, PARTNER, "second");
    engine.execute_next_intention().unwrap().unwrap();
    assert_eq!(
        bandages(&engine, PLAYER),
        1,
        "preparation is an unpaid hold"
    );
    assert_eq!(cooldown(&engine, PLAYER), 1200);
    assert_eq!(
        engine.state(PARTNER).unwrap().observation.ground_items,
        public
    );
    let failed = engine.execute_next_intention().unwrap().unwrap();
    assert!(matches!(
        failed.entry.content,
        JournalContent::IntentionFailed { .. }
    ));
    assert_eq!(cooldown(&engine, PARTNER), 0);
    assert!(combat(&engine, PARTNER).preparation_remaining.is_none());
    finish(&mut engine, PLAYER);
    assert_eq!(bandages(&engine, PLAYER), 0);
    assert_eq!(
        combat(&engine, PLAYER).hp,
        combat(&engine, PLAYER).max_hp,
        "zero-effect resolution still spends once"
    );
    assert!(!engine.state(PLAYER).unwrap().wizard_game);
}

#[test]
fn cancelling_unpaid_claim_allows_other_actor_without_refunding_cooldown() {
    let mut engine = Engine::memory(scenario(1, 1, Some(false), 1)).unwrap();
    let (admitted, _) = admit(&mut engine, PLAYER, "first");
    engine.execute_next_intention().unwrap().unwrap();
    let cancel = Command::CancelIntention {
        expected_boundary: engine.causal_boundary(PLAYER).unwrap(),
        admission: admitted.entry.id,
        step: None,
    };
    let cancelled = submit(&mut engine, PLAYER, "cancel", cancel.clone());
    assert!(cancelled
        .entry
        .intention_ends
        .iter()
        .any(|end| end.kind == IntentionEndKind::Cancelled));
    assert_eq!(cooldown(&engine, PLAYER), 1200);
    assert_eq!(bandages(&engine, PLAYER), 1);
    let before = engine.state(PLAYER).unwrap();
    assert!(submit(&mut engine, PLAYER, "cancel", cancel).duplicate);
    assert_eq!(engine.state(PLAYER).unwrap(), before);
    support::act_as(&mut engine, PLAYER, Action::Wait).unwrap();
    admit(&mut engine, PARTNER, "other");
    engine.execute_next_intention().unwrap().unwrap();
    assert!(combat(&engine, PARTNER).preparation_remaining.is_some());
    finish(&mut engine, PARTNER);
    assert_eq!(bandages(&engine, PLAYER), 0);
}

#[test]
fn saved_claim_resumes_once_and_spent_source_retry_needs_no_live_item() {
    for checkpoint_interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("supplies.db");
        let mut engine = Engine::open_with_policy(
            &path,
            scenario(1, 1, Some(false), 1),
            SavePolicy {
                checkpoint_interval,
                ..Default::default()
            },
        )
        .unwrap();
        let (admitted, request) = admit(&mut engine, PLAYER, "care");
        engine.execute_next_intention().unwrap().unwrap();
        let held = engine.state(PLAYER).unwrap();
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open(&path, Scenario::two_room(99)).unwrap();
        assert_eq!(engine.state(PLAYER).unwrap(), held);
        engine.pause_preparation(PLAYER).unwrap().unwrap();
        let paused = engine.state(PLAYER).unwrap();
        assert_eq!(bandages(&engine, PLAYER), 1);
        let retry = submit(&mut engine, PLAYER, "care", request.clone());
        assert!(retry.duplicate);
        assert_eq!(retry.entry, admitted.entry);
        assert_eq!(engine.state(PLAYER).unwrap(), paused);
        let resume = Command::ResumeIntention {
            expected_boundary: engine.causal_boundary(PLAYER).unwrap(),
            admission: admitted.entry.id.clone(),
        };
        submit(&mut engine, PLAYER, "resume", resume);
        engine.execute_next_intention().unwrap().unwrap();
        finish(&mut engine, PLAYER);
        assert_eq!(bandages(&engine, PLAYER), 0);
        let spent = engine.state(PLAYER).unwrap();
        assert!(submit(&mut engine, PLAYER, "care", request.clone()).duplicate);
        assert_eq!(engine.state(PLAYER).unwrap(), spent);
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open(&path, Scenario::two_room(123)).unwrap();
        assert_eq!(engine.state(PLAYER).unwrap(), spent);
        assert!(submit(&mut engine, PLAYER, "care", request).duplicate);
        assert_eq!(engine.state(PLAYER).unwrap(), spent);
    }
}

#[test]
fn stale_exact_ground_source_rejection_is_atomic_after_ordinary_pickup() {
    let mut engine = Engine::memory(scenario(1, 1, Some(false), 1)).unwrap();
    support::act_as(
        &mut engine,
        PLAYER,
        Action::Move {
            direction: tor_server::journal::Direction::East,
        },
    )
    .unwrap();
    support::act_as(&mut engine, PARTNER, Action::Wait).unwrap();
    support::act_as(
        &mut engine,
        PLAYER,
        Action::Take {
            item: 10,
            quantity: None,
        },
    )
    .unwrap();
    let before = engine.state(PARTNER).unwrap();
    let before_boundary = engine.causal_boundary(PARTNER).unwrap();
    let counts = engine.profile_counts();
    let command = Command::AdmitIntention {
        expected_boundary: before_boundary,
        action: care(PARTNER),
    };
    assert!(engine
        .command(
            "player",
            "supply-test",
            PARTNER,
            "stale",
            &engine.branch().clone(),
            command
        )
        .is_err());
    assert_eq!(engine.state(PARTNER).unwrap(), before);
    assert_eq!(engine.profile_counts(), counts);
    assert_eq!(cooldown(&engine, PARTNER), 0);
}

fn tick(engine: &Engine) -> u64 {
    engine.state(PLAYER).unwrap().observation.tick
}

fn ordinary_action(engine: &mut Engine, action: Action, serial: &mut u64) -> CommandResult {
    *serial += 1;
    let actor = PLAYER;
    let request = Command::AdmitIntention {
        expected_boundary: engine.causal_boundary(actor).unwrap(),
        action,
    };
    let admitted = submit(engine, actor, &format!("measurement-{serial}"), request);
    // This is the session's ordinary due-work path, not a direct effects kernel.
    loop {
        if let Some(executed) = engine.execute_next_intention().unwrap() {
            assert_eq!(executed.entry.actor, actor);
            break;
        }
        step(engine);
    }
    admitted
}

fn injure(engine: &mut Engine, serial: &mut u64) {
    ordinary_action(engine, Action::Drink { item: 11 }, serial);
    finish(engine, PLAYER);
    assert!(combat(engine, PLAYER).hp < combat(engine, PLAYER).max_hp);
    assert!(!engine.state(PLAYER).unwrap().wizard_game);
}

fn rest_measurement(hd: usize, stock: u64) -> Value {
    let preview = Engine::memory(scenario(hd, stock, Some(false), 1)).unwrap();
    let injury = combat(&preview, PLAYER).max_hp / 2;
    assert!(injury > 0);
    let mut engine = Engine::memory(scenario(
        hd,
        stock,
        Some(false),
        u32::try_from(injury).unwrap(),
    ))
    .unwrap();
    let mut serial = 0;
    injure(&mut engine, &mut serial);
    let initial = combat(&engine, PLAYER);
    let start = tick(&engine);
    let mut uses = 0;
    let mut explicit = 0;
    let mut natural = 0;
    let mut rows = Vec::new();
    for _ in 0..2000 {
        let before = combat(&engine, PLAYER);
        if before.hp == before.max_hp {
            break;
        }
        let before_tick = tick(&engine);
        let before_stock = bandages(&engine, PLAYER);
        // The second ordinary controlled actor waits every 100 ticks, bounding
        // each observed interval and exposing the 300-tick resolution boundary
        // before the following recovery. Full-Health normalization is therefore
        // never confused with later natural recovery after explicit healing.
        if engine.next_actor() == Some(PLAYER) && before.preparation_remaining.is_none() {
            let action = if before_stock > 0 && cooldown(&engine, PLAYER) == 0 {
                care(PLAYER)
            } else {
                Action::Wait
            };
            ordinary_action(&mut engine, action, &mut serial);
        } else {
            step(&mut engine);
        }
        let after = combat(&engine, PLAYER);
        let elapsed = tick(&engine) - before_tick;
        let recovery = before.own_stats.as_ref().unwrap().health_recovery;
        let recovered = ((recovery.elapsed_ticks + elapsed) / recovery.period_ticks)
            .min(before.max_hp - before.hp);
        let spent = before_stock - bandages(&engine, PLAYER);
        assert!(spent <= 1);
        let healed = after
            .hp
            .checked_sub(before.hp + recovered)
            .expect("rest must separate natural and explicit healing");
        assert!(healed <= 2 * spent);
        if spent == 0 {
            assert_eq!(healed, 0);
        }
        uses += spent;
        explicit += healed;
        natural += recovered;
        rows.push(json!({"tick":tick(&engine),"hp":after.hp,"bandages":bandages(&engine,PLAYER),
            "natural_heal":recovered,"explicit_heal":healed,"spent":spent,"cooldown":cooldown(&engine,PLAYER)}));
    }
    let final_health = combat(&engine, PLAYER);
    assert_eq!(
        final_health.hp, final_health.max_hp,
        "bounded ordinary waits reach full Health"
    );
    assert_eq!(explicit + natural, initial.max_hp - initial.hp);
    assert_eq!(uses, stock - bandages(&engine, PLAYER));
    if stock == 0 {
        assert_eq!((uses, explicit), (0, 0));
    } else {
        assert!(uses > 0);
        assert!(explicit > 0);
    }
    json!({"case":"uncontested_rest","hd":hd,"initial_bandages":stock,
        "initial_hp":initial.hp,"maximum_hp":initial.max_hp,
        "natural_period":initial.own_stats.unwrap().health_recovery.period_ticks,
        "active_ticks_to_full":tick(&engine)-start,"finite_uses":uses,
        "explicit_heal":explicit,"natural_heal":natural,"rows":rows})
}

fn pressure_measurement(hd: usize, stock: u64) -> Value {
    let preview = Engine::memory(scenario(hd, stock, Some(true), 1)).unwrap();
    let injury = combat(&preview, PLAYER).max_hp / 2;
    let mut engine = Engine::memory(scenario(
        hd,
        stock,
        Some(true),
        u32::try_from(injury).unwrap(),
    ))
    .unwrap();
    let mut serial = 0;
    injure(&mut engine, &mut serial);
    let initial = combat(&engine, PLAYER);
    let start = tick(&engine);
    let mut admission = None;
    let mut attempts = 0;
    let mut interrupted = 0;
    let mut damage_received = 0;
    let mut rows = Vec::new();
    // A two-thousand-active-tick exposure, or death, is the measurement boundary.
    // This ordinary foe attacks every 200 ticks; its damage repeatedly interrupts
    // the provisional 300-tick preparation before a bandage can be spent.
    for _ in 0..1000 {
        if !engine.alive(PLAYER) || tick(&engine) - start >= 2000 {
            break;
        }
        let before = combat(&engine, PLAYER);
        let before_tick = tick(&engine);
        let recovery_before = before.own_stats.as_ref().unwrap().health_recovery;
        if before.preparation_remaining.is_some() && !before.preparation_active {
            interrupted += 1;
            let source = admission
                .take()
                .expect("interrupted ordinary care admission");
            let command = Command::CancelIntention {
                expected_boundary: engine.causal_boundary(PLAYER).unwrap(),
                admission: source,
                step: None,
            };
            serial += 1;
            submit(
                &mut engine,
                PLAYER,
                &format!("pressure-cancel-{serial}"),
                command,
            );
        } else if engine.next_actor() == Some(PLAYER)
            && before.preparation_remaining.is_none()
            && stock > 0
            && cooldown(&engine, PLAYER) == 0
        {
            let result = ordinary_action(&mut engine, care(PLAYER), &mut serial);
            admission = Some(result.entry.id);
            attempts += 1;
        } else {
            step(&mut engine);
        }
        let after = combat(&engine, PLAYER);
        let active_delta = tick(&engine) - before_tick;
        let recovery_upper_bound = recovery_before.elapsed_ticks + active_delta;
        // These actors remain in the active region. Damage only resets this
        // unchanged recovery clock, so even ignoring every reset must stay below
        // the first boundary. Net damage alone cannot establish zero recovery.
        assert!(
            recovery_upper_bound < recovery_before.period_ticks,
            "pressure interval could conceal natural recovery offset by damage"
        );
        damage_received += before.hp.saturating_sub(after.hp);
        assert!(
            after.hp <= before.hp,
            "pressure fixture must retain any unexpected healing as a failure"
        );
        // The living foe remains beside the physical ground stack even if the
        // measured actor dies and loses its own useful observations.
        assert_eq!(
            bandages(&engine, PARTNER),
            stock,
            "interrupted preparation does not spend"
        );
        rows.push(json!({"tick":tick(&engine),"hp":after.hp,"bandages":bandages(&engine,PARTNER),
            "preparation_active":after.preparation_active,"cooldown":cooldown(&engine,PLAYER),
            "recovery_elapsed_before":recovery_before.elapsed_ticks,"active_tick_delta":active_delta,
            "recovery_period":recovery_before.period_ticks,"recovery_upper_bound":recovery_upper_bound}));
    }
    assert!(
        damage_received > 0,
        "ordinary hostile AI must exert actual pressure"
    );
    if stock > 0 {
        assert!(attempts > 0);
        assert!(interrupted > 0 || !engine.alive(PLAYER));
    } else {
        assert_eq!(attempts, 0);
    }
    json!({"case":"hostile_pressure","hd":hd,"initial_bandages":stock,
        "initial_hp":initial.hp,"maximum_hp":initial.max_hp,"final_hp":combat(&engine,PLAYER).hp,
        "active_ticks":tick(&engine)-start,"horizon_ticks":2000,"attempts":attempts,
        "suspended_attempts":interrupted,"dead":!engine.alive(PLAYER),"damage_received":damage_received,
        "finite_uses":0,"explicit_heal":0,"natural_heal":0,"rows":rows})
}

#[test]
fn bind_wounds_provisional_measurements_use_ordinary_engine_actions() {
    let mut records = Vec::new();
    for hd in [1, 6, 11] {
        for bandages in [0, 3] {
            records.push(rest_measurement(hd, bandages));
            records.push(pressure_measurement(hd, bandages));
        }
    }
    if let Some(path) = std::env::var_os("TOR_BIND_WOUNDS_MEASUREMENTS") {
        let mut file = std::fs::File::create(path).unwrap();
        for record in records {
            serde_json::to_writer(&mut file, &record).unwrap();
            writeln!(file).unwrap();
        }
    }
}

fn contention_scenario() -> Scenario {
    let base = scenario(1, 1, Some(true), 1);
    let maximum = combat(&Engine::memory(base.clone()).unwrap(), PARTNER).max_hp;
    let package = base.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    // Isolate supply contention from retained combat work: the carer waits
    // before injury, then chooses generic care even after the strike provokes it.
    manifest["creatures"]["species"]["carer"]["behavior"] =
        json!({"disposition":"passive","idle":"wait","memory_ticks":1000});
    let mut species = manifest["creatures"]["species"]["carer"].clone();
    species["modifiers"] =
        json!([{"scope":"always","op":{"kind":"grant_ability","ability":"fixture_injury"}}]);
    manifest["creatures"]["species"]["injurer"] = species;
    let mut regions = package.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["actors"][0]["at"] = json!([3, 1, 0]);
    let mut injurer = region["actors"][0].clone();
    injurer["id"] = json!(3);
    injurer["at"] = json!([4, 1, 0]);
    injurer["controller"] = json!("external");
    injurer["ai"] = Value::Null;
    injurer["creature"]["species"] = json!("injurer");
    injurer["creature"]["faction"] = json!("carers");
    injurer["creature"]["name"] = json!("ordinary sparring partner");
    region["actors"].as_array_mut().unwrap().push(injurer);
    regions[0] = serde_json::from_value(region).unwrap();
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    rules.abilities.push(serde_json::from_value(json!({"id":"fixture_injury","name":"Fixture Injury",
        "tags":[],"basic_attack":false,"timing":{"source":{"kind":"fixed","prepare":1,"recover":1000}},
        "reach":{"kind":"sight","range":6},"strikes":{"kind":"authored","strikes":[strike(u32::try_from(maximum/2+1).unwrap())]},
        "interaction": "hostile_to_target", "ai":{"priority":1,"hostile":true,"minimum_resource_percent":0,"range_preference":"any"}})).unwrap());
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        regions,
        &toml::to_string(&rules).unwrap(),
    )
    .unwrap();
    Scenario {
        package: Some(Arc::new(package)),
        ..base
    }
}

#[test]
fn ordinary_ai_loses_held_last_bandage_then_heals_when_human_cancels() {
    let mut engine = Engine::memory(contention_scenario()).unwrap();
    let (human, _) = admit(&mut engine, PLAYER, "human-care");
    engine.execute_next_intention().unwrap().unwrap();
    assert_eq!(engine.next_actor(), Some(PARTNER));
    assert_eq!(engine.next_ai_action(), Some((PARTNER, Action::Wait)));
    engine.advance_ai(PARTNER).unwrap();
    assert!(combat(&engine, PARTNER).preparation_remaining.is_none());
    assert_eq!(engine.next_actor(), Some(ActorId(3)));
    support::act_as(
        &mut engine,
        ActorId(3),
        Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new("fixture_injury").unwrap(),
            invocation: tor_server::journal::AbilityInvocation {
                aim: tor_server::journal::AbilityAim::ActorTarget {
                    target: tor_simulation::ActorId(2),
                },
                weapon: tor_server::journal::WeaponSelection::Default,
            },
            supplies: SupplySelection::Carried,
        },
    )
    .unwrap();
    assert!(combat(&engine, PARTNER).hp * 2 <= combat(&engine, PARTNER).max_hp);
    assert_eq!(engine.next_ai_action(), Some((PARTNER, care(PARTNER))));
    let held_facts = engine.state(PARTNER).unwrap().observation.ground_items;
    let loser = engine.advance_ai(PARTNER).unwrap();
    assert!(matches!(
        loser.entry.content,
        JournalContent::IntentionStarted {
            action: Action::Wait,
            ..
        }
    ));
    assert_eq!(cooldown(&engine, PARTNER), 0);
    assert_eq!(bandages(&engine, PLAYER), 1);
    let command = Command::CancelIntention {
        expected_boundary: engine.causal_boundary(PLAYER).unwrap(),
        admission: human.entry.id,
        step: None,
    };
    submit(&mut engine, PLAYER, "release-last-bandage", command);
    assert_eq!(
        engine.state(PARTNER).unwrap().observation.ground_items,
        held_facts
    );
    while engine.next_actor() != Some(PARTNER) {
        step(&mut engine);
    }
    assert_eq!(engine.next_ai_action(), Some((PARTNER, care(PARTNER))));
    let before = combat(&engine, PARTNER).hp;
    let winner = engine.advance_ai(PARTNER).unwrap();
    assert!(matches!(
        winner.entry.content,
        JournalContent::IntentionStarted {
            action: Action::UseAbility { .. },
            ..
        }
    ));
    assert!(cooldown(&engine, PARTNER) > 0);
    finish(&mut engine, PARTNER);
    assert_eq!(bandages(&engine, PLAYER), 0);
    assert!(combat(&engine, PARTNER).hp > before);
    assert!(!engine.state(PLAYER).unwrap().wizard_game);
}

fn paid_care_scenario() -> Scenario {
    let base = scenario(1, 1, Some(false), 1);
    let package = base.package.as_ref().unwrap();
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    let mut ability = serde_json::to_value(
        rules
            .abilities
            .iter()
            .find(|ability| ability.id.as_str() == "bind_wounds")
            .unwrap(),
    )
    .unwrap();
    ability["id"] = json!("paid_care");
    ability["name"] = json!("Paid Care");
    ability["cost"] = json!({"resource":"stamina","start":1,"resolution":1});
    rules
        .abilities
        .push(serde_json::from_value(ability).unwrap());
    rules
        .constants
        .default_abilities
        .push(tor_simulation::content_ids::AbilityId::new("paid_care").unwrap());
    let package = Package::from_parts(
        package.manifest.clone(),
        package.region_defs().unwrap(),
        &toml::to_string(&rules).unwrap(),
    )
    .unwrap();
    Scenario {
        package: Some(Arc::new(package)),
        ..base
    }
}

#[test]
fn late_store_rejection_preserves_queued_supply_start_and_exact_saved_retry() {
    for checkpoint_interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rejected-start.db");
        let policy = SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let fixture = paid_care_scenario();
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
        let mut action = care(PLAYER);
        let Action::UseAbility { id, .. } = &mut action else {
            unreachable!()
        };
        *id = tor_simulation::content_ids::AbilityId::new("paid_care").unwrap();
        let request = Command::AdmitIntention {
            expected_boundary: engine.causal_boundary(PLAYER).unwrap(),
            action,
        };
        let admitted = submit(&mut engine, PLAYER, "paid-care", request.clone());
        engine.flush().unwrap();
        let saved = tor_server::inspect_save(&path).unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(
            &path,
            fixture.clone(),
            SavePolicy {
                max_pending_bytes: 1,
                ..policy.clone()
            },
        )
        .unwrap();
        let before = [PLAYER, PARTNER].map(|actor| engine.state(actor).unwrap());
        let histories =
            [PLAYER, PARTNER].map(|actor| engine.history(actor, "player", None, 50).unwrap());
        let counts = engine.profile_counts();
        let accepted_sequence = engine.save_status().accepted_sequence;
        for _ in 0..2 {
            // Execution builds a changed private candidate before Store rejects
            // publication. This is deliberately later than action preflight.
            let error = engine.execute_next_intention().unwrap_err();
            assert_eq!(error.code, tor_protocol::ErrorCode::StorageFailure);
            assert_eq!(
                error.message,
                "Save queue is full; wait for saving to finish before retrying"
            );
            assert_eq!(
                [PLAYER, PARTNER].map(|actor| engine.state(actor).unwrap()),
                before
            );
            assert_eq!(
                [PLAYER, PARTNER].map(|actor| engine.history(actor, "player", None, 50).unwrap()),
                histories
            );
            assert_eq!(engine.profile_counts(), counts);
            assert_eq!(engine.save_status().accepted_sequence, accepted_sequence);
            assert_eq!(engine.save_status().durable_sequence, accepted_sequence);
            let duplicate = submit(&mut engine, PLAYER, "paid-care", request.clone());
            assert!(duplicate.duplicate);
            assert_eq!(duplicate.entry, admitted.entry);
            assert_eq!(tor_server::inspect_save(&path).unwrap(), saved);
        }
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, fixture, policy).unwrap();
        let initial = combat(&engine, PLAYER).own_stats.unwrap().resources;
        let start = engine.execute_next_intention().unwrap().unwrap();
        assert!(matches!(
            start.entry.content,
            JournalContent::IntentionStarted { .. }
        ));
        assert_eq!(bandages(&engine, PLAYER), 1);
        let stats = combat(&engine, PLAYER).own_stats.unwrap();
        let before_stamina = initial
            .iter()
            .find(|row| row.resource.id == "stamina")
            .unwrap();
        let stamina = stats
            .resources
            .iter()
            .find(|row| row.resource.id == "stamina")
            .unwrap();
        assert_eq!(stamina.balance + 1, before_stamina.balance);
        assert_eq!(stamina.reserved, 1);
        assert_eq!(
            stats
                .abilities
                .iter()
                .find(|ability| ability.id == "paid_care")
                .unwrap()
                .cooldown_remaining,
            1200
        );
        finish(&mut engine, PLAYER);
        assert_eq!(bandages(&engine, PLAYER), 0);
        let completed = engine.state(PLAYER).unwrap();
        assert!(submit(&mut engine, PLAYER, "paid-care", request.clone()).duplicate);
        assert_eq!(engine.state(PLAYER).unwrap(), completed);
        engine.flush().unwrap();
        drop(engine);
        let mut reopened = Engine::open(&path, Scenario::two_room(99)).unwrap();
        assert_eq!(reopened.state(PLAYER).unwrap(), completed);
        assert!(submit(&mut reopened, PLAYER, "paid-care", request).duplicate);
        assert_eq!(bandages(&reopened, PLAYER), 0);
    }
}

#[test]
fn unowned_backend_drink_is_atomic_but_ordinary_admission_spends_once() {
    let mut engine = Engine::memory(scenario(1, 0, Some(false), 1)).unwrap();
    let before = [PLAYER, PARTNER].map(|actor| engine.state(actor).unwrap());
    let before_boundary = engine.causal_boundary(PLAYER).unwrap();
    let histories =
        [PLAYER, PARTNER].map(|actor| engine.history(actor, "player", None, 50).unwrap());
    let save_status = serde_json::to_value(engine.save_status()).unwrap();
    let counts = engine.profile_counts();
    let dose = engine.target_scope(PLAYER).item(tor_simulation::ItemId(11));
    assert!(before[0]
        .observation
        .inventory
        .iter()
        .any(|item| item.id == dose && item.quantity == 1));
    let unowned = Command::Act {
        expected_boundary: before_boundary,
        action: Action::Drink { item: 11 },
    };
    assert!(engine
        .command(
            "player",
            "supply-test",
            PLAYER,
            "drink",
            &engine.branch().clone(),
            unowned
        )
        .is_err());
    assert_eq!(
        [PLAYER, PARTNER].map(|actor| engine.state(actor).unwrap()),
        before
    );
    assert_eq!(
        [PLAYER, PARTNER].map(|actor| engine.history(actor, "player", None, 50).unwrap()),
        histories
    );
    assert_eq!(
        serde_json::to_value(engine.save_status()).unwrap(),
        save_status
    );
    assert_eq!(engine.profile_counts(), counts);
    // The rejected direct call left no receipt; the real wire path admits this
    // ordinary finite-item command and supplies its durable intention identity.
    let request = Command::AdmitIntention {
        expected_boundary: before_boundary,
        action: Action::Drink { item: 11 },
    };
    let admitted = submit(&mut engine, PLAYER, "drink", request.clone());
    engine.execute_next_intention().unwrap().unwrap();
    finish(&mut engine, PLAYER);
    let completed = engine.state(PLAYER).unwrap();
    assert!(!completed
        .observation
        .inventory
        .iter()
        .any(|item| item.id == dose));
    assert_eq!(
        combat(&engine, PLAYER).hp + 1,
        before[0].observation.combat.as_ref().unwrap().hp
    );
    let retry = submit(&mut engine, PLAYER, "drink", request);
    assert!(retry.duplicate);
    assert_eq!(retry.entry, admitted.entry);
    assert_eq!(engine.state(PLAYER).unwrap(), completed);
    assert!(engine.execute_next_intention().unwrap().is_none());
    assert!(!completed.wizard_game);
}
