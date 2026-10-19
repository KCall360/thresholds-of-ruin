//! A form is one immutable acquisition operand owned by its talent's HD purchase.
use super::creature_fixture as fixture;
use std::collections::BTreeMap;
use tor_simulation::{
    attributes::{ManaBinding, Skill},
    content_ids::{AbilityId, FormId, TalentId},
    creatures::{
        record::CreatureRecord, AdvancementPurchase, Content, CreatureBuild, CreatureState,
        PurchaseLocation, PurchaseSelection, ScenarioCatalog,
    },
    progression::{HdLedger, HdSource},
    rules::{FormDef, Requirement, Ruleset, TalentDef},
};
fn talent() -> TalentId {
    TalentId::new("form_root").unwrap()
}
fn form(id: &str) -> FormId {
    FormId::new(id).unwrap()
}
fn ability(id: &str) -> AbilityId {
    AbilityId::new(id).unwrap()
}
fn build() -> CreatureBuild {
    let mut document = fixture::ruleset().document().clone();
    let base = document
        .abilities
        .iter()
        .find(|row| row.basic_attack)
        .unwrap()
        .clone();
    for id in ["first_form", "second_form", "foreign_form"] {
        let mut selected = base.clone();
        selected.id = ability(id);
        selected.name = id.into();
        selected.basic_attack = false;
        document.abilities.push(selected);
        document.forms.push(FormDef {
            id: form(id),
            name: id.into(),
            ability: ability(id),
        });
    }
    document.talents.push(TalentDef {
        id: talent(),
        name: "Choose a form".into(),
        prerequisites: vec![],
        requirement: Requirement::TotalHd {
            minimum: tor_simulation::HdCount(2),
        },
        modifiers: vec![],
        form_choices: vec![form("first_form"), form("second_form")],
        expertise_grants: vec![],
    });
    let species = fixture::species();
    let id = species.id.clone();
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: BTreeMap::from([(id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    CreatureBuild::new(
        content,
        id,
        HdLedger::seeded(vec![HdSource::Racial; 2], 42).unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap()
}
fn choose(
    build: &mut CreatureBuild,
    owner: usize,
    selected: &str,
) -> Result<(), tor_simulation::creatures::BuildError> {
    build.purchase(
        tor_simulation::HdIndex::from_usize(owner).unwrap(),
        tor_simulation::content_ids::PointPoolId::new("advancement").unwrap(),
        PurchaseSelection::TalentForm {
            talent: talent(),
            form: form(selected),
        },
    )
}
#[test]
fn a_choice_requiring_root_rejects_an_ordinary_purchase_atomically() {
    let mut build = build();
    let before = build.clone();
    assert!(build
        .select_talent(tor_simulation::HdIndex(0), talent())
        .is_err());
    assert_eq!(build, before);
    assert_eq!(build.unspent_points(), 2);
}
#[test]
fn selected_form_grants_only_its_immutable_ability_under_the_root_provenance() {
    let mut build = build();
    choose(&mut build, 0, "first_form").unwrap();
    let derived = build.derive().unwrap();
    assert!(derived.abilities.contains(&ability("first_form")));
    assert!(!derived.abilities.contains(&ability("second_form")));
    assert!(!derived.abilities.contains(&ability("foreign_form")));
    assert_eq!(
        build.choices()[0].talents().collect::<Vec<_>>(),
        vec![&talent()]
    );
    assert_eq!(build.unspent_points(), 1);
    assert_eq!(
        derived
            .sources
            .keys()
            .filter(|source| matches!(source, tor_simulation::creatures::SourceId::Talent(_)))
            .count(),
        1
    );
}
#[test]
fn foreign_form_and_duplicate_root_choices_cannot_spend_or_change_an_owned_form() {
    let mut build = build();
    let before = build.clone();
    assert!(choose(&mut build, 0, "foreign_form").is_err());
    assert_eq!(build, before);
    choose(&mut build, 0, "first_form").unwrap();
    let before = build.clone();
    assert!(choose(&mut build, 1, "second_form").is_err());
    assert_eq!(build, before);
}
#[test]
fn draining_eligibility_makes_the_chosen_form_dormant_without_replacing_it() {
    let mut build = build();
    choose(&mut build, 0, "second_form").unwrap();
    let choice = build.choices()[0].clone();
    build.remove_latest().unwrap();
    assert_eq!(build.choices()[0], choice);
    let derived = build.derive().unwrap();
    assert!(derived.dormant_talents.contains(&talent()));
    assert!(!derived.abilities.contains(&ability("second_form")));
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    assert_eq!(build.choices()[0], choice);
    assert!(build
        .derive()
        .unwrap()
        .abilities
        .contains(&ability("second_form")));
}
#[test]
fn explicit_same_root_form_replacement_preserves_one_purchase_and_runtime_state() {
    let mut build = build();
    choose(&mut build, 0, "first_form").unwrap();
    let mut creature = CreatureState::new(build).unwrap();
    creature.damage(3);
    creature.advance_active(333);
    let before_health = creature.health();
    let before_recovery = creature.health_recovery();
    let before_resources = creature.costs().resources().clone();
    let replacement = AdvancementPurchase {
        pool: tor_simulation::content_ids::PointPoolId::new("advancement").unwrap(),
        selection: PurchaseSelection::TalentForm {
            talent: talent(),
            form: form("second_form"),
        },
    };
    let preview = creature
        .build()
        .preview_replacement(
            PurchaseLocation {
                owner: tor_simulation::HdIndex(0),
                index: 0,
            },
            Some(replacement),
        )
        .unwrap();
    assert_eq!(preview.build().unspent_points(), 1);
    creature.rebuild(preview.build().clone()).unwrap();
    assert_eq!(creature.health(), before_health);
    assert_eq!(creature.health_recovery(), before_recovery);
    assert_eq!(creature.costs().resources(), &before_resources);
    assert!(!creature
        .derived()
        .abilities
        .contains(&ability("first_form")));
    assert!(creature
        .derived()
        .abilities
        .contains(&ability("second_form")));
}
#[test]
fn chosen_form_survives_named_record_restore_without_ability_or_rng_reselection() {
    let mut build = build();
    choose(&mut build, 0, "second_form").unwrap();
    let mut creature = CreatureState::new(build).unwrap();
    creature.damage(3);
    creature.advance_active(333);
    let bytes = serde_json::to_vec(&CreatureRecord::capture(&creature)).unwrap();
    assert!(String::from_utf8(bytes.clone())
        .unwrap()
        .contains(r#""kind":"talent_form","talent":"form_root","form":"second_form""#));
    let record: CreatureRecord = serde_json::from_slice(&bytes).unwrap();
    let restored = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, creature);
    let duplicate = String::from_utf8(bytes.clone()).unwrap().replace(
        r#""form":"second_form""#,
        r#""form":"second_form","form":"first_form""#,
    );
    assert!(serde_json::from_slice::<CreatureRecord>(duplicate.as_bytes()).is_err());
    // Test actual byte decoding, independently from the readable serialization control.
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for selection in [
        serde_json::json!({"kind":"talent_form","talent":"form_root"}),
        serde_json::json!({"kind":"talent_form","talent":"form_root","form":null}),
        serde_json::json!({"kind":"talent_form","talent":"form_root","form":"second_form","extra":0}),
        serde_json::json!(["talent_form", "form_root", "second_form"]),
    ] {
        let mut value = original.clone();
        value["build"]["choices"][0]["purchases"][0]["selection"] = selection;
        assert!(
            serde_json::from_slice::<CreatureRecord>(&serde_json::to_vec(&value).unwrap()).is_err()
        );
    }
}

#[test]
fn invalid_or_ordinary_root_form_operands_reject_without_spending() {
    let original = build();
    let ordinary = original
        .ruleset()
        .document()
        .talents
        .iter()
        .find(|root| root.form_choices.is_empty())
        .unwrap()
        .id
        .clone();
    for selection in [
        PurchaseSelection::TalentForm {
            talent: talent(),
            form: form("unknown_form"),
        },
        PurchaseSelection::TalentForm {
            talent: ordinary,
            form: form("first_form"),
        },
        PurchaseSelection::TalentForm {
            talent: TalentId::new("unknown_root").unwrap(),
            form: form("first_form"),
        },
    ] {
        let mut candidate = original.clone();
        let result = candidate.purchase(
            tor_simulation::HdIndex(0),
            tor_simulation::content_ids::PointPoolId::new("advancement").unwrap(),
            selection,
        );
        assert!(result.is_err());
        assert_eq!(candidate, original);
    }
}

#[test]
fn dormant_record_restore_still_validates_the_exact_root_form_operand() {
    let mut build = build();
    choose(&mut build, 0, "second_form").unwrap();
    build.remove_latest().unwrap();
    let creature = CreatureState::new(build).unwrap();
    assert!(creature.derived().dormant_talents.contains(&talent()));
    let value = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
    let control: CreatureRecord =
        serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        control
            .restore(creature.build().content().clone(), vec![])
            .unwrap(),
        creature
    );
    for selection in [
        serde_json::json!({"kind":"talent_form","talent":"form_root","form":"foreign_form"}),
        serde_json::json!({"kind":"talent_form","talent":"form_root","form":"unknown_form"}),
        serde_json::json!({"kind":"talent","talent":"form_root"}),
    ] {
        let mut bad = value.clone();
        bad["build"]["choices"][0]["purchases"][0]["selection"] = selection;
        let record: CreatureRecord =
            serde_json::from_slice(&serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(record
            .restore(creature.build().content().clone(), vec![])
            .is_err());
    }
}

#[test]
fn lifecycle_authored_hd_restoration_keeps_the_chosen_form_and_recorded_health_rolls() {
    use tor_simulation::creatures::{BuildAdvancement, PlannedAdvancement, RestorationPolicy};
    let mut original = build();
    choose(&mut original, 1, "second_form").unwrap();
    let authored = BuildAdvancement {
        starting_skills: *original.starting_skills(),
        choices: original.choices().to_vec(),
        restoration: RestorationPolicy::Authored(
            original
                .ledger()
                .entries()
                .iter()
                .zip(original.choices())
                .map(|(entry, choices)| PlannedAdvancement {
                    source: entry.source(),
                    choices: choices.clone(),
                })
                .collect(),
        ),
    };
    let mut build = CreatureBuild::from_recorded(
        original.content().clone(),
        original.species_id().clone(),
        original.initial_attributes(),
        original.ledger().clone(),
        original.binding(),
        authored,
        vec![],
    )
    .unwrap();
    let before = build.clone();
    let removed = build.remove_latest().unwrap();
    assert_eq!(
        removed.choices.purchases[0].selection.form(),
        Some(&form("second_form"))
    );
    assert!(!build
        .derive()
        .unwrap()
        .abilities
        .contains(&ability("second_form")));
    build.restore_hit_die(42).unwrap();
    assert_eq!(build, before);
    assert!(build
        .derive()
        .unwrap()
        .abilities
        .contains(&ability("second_form")));
}

#[test]
fn lifecycle_explicit_form_replacement_keeps_existing_cooldowns_and_replay_fraction() {
    let mut build = build();
    choose(&mut build, 0, "first_form").unwrap();
    let mut creature = CreatureState::new(build).unwrap();
    creature.damage(4);
    creature.advance_active(313);
    let mut value = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
    value["cooldowns"] = serde_json::json!([{"ability":"first_form","remaining":79}]);
    let record: CreatureRecord =
        serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    creature = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    let before = creature.clone();
    let replacement = AdvancementPurchase {
        pool: tor_simulation::content_ids::PointPoolId::new("advancement").unwrap(),
        selection: PurchaseSelection::TalentForm {
            talent: talent(),
            form: form("second_form"),
        },
    };
    let preview = creature
        .build()
        .preview_replacement(
            PurchaseLocation {
                owner: tor_simulation::HdIndex(0),
                index: 0,
            },
            Some(replacement),
        )
        .unwrap();
    creature.rebuild(preview.build().clone()).unwrap();
    assert_eq!(creature.health_recovery(), before.health_recovery());
    assert_eq!(creature.cooldowns(), before.cooldowns());
    let bytes = serde_json::to_vec(&CreatureRecord::capture(&creature)).unwrap();
    let record: CreatureRecord = serde_json::from_slice(&bytes).unwrap();
    let mut replay = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    creature.advance_active(79);
    replay.advance_active(79);
    assert_eq!(creature, replay);
    assert_eq!(creature.cooldowns().next_expiry_in(), None);
    assert!(!creature
        .derived()
        .abilities
        .contains(&ability("first_form")));
    assert!(creature
        .derived()
        .abilities
        .contains(&ability("second_form")));
}

#[test]
fn lifecycle_changing_a_form_preserves_root_dependents_but_refunding_the_root_removes_them() {
    use tor_simulation::creatures::record::BuildRecord;
    let mut original = build();
    original.add_hit_die(HdSource::Racial, 42).unwrap();
    let child = TalentId::new("form_child").unwrap();
    let mut document = original.ruleset().document().clone();
    document.talents.push(TalentDef {
        id: child.clone(),
        name: "Form child".into(),
        prerequisites: vec![],
        requirement: Requirement::OwnedTalent { talent: talent() },
        modifiers: vec![],
        form_choices: vec![],
        expertise_grants: vec![],
    });
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        original.content().catalog().clone(),
    )
    .unwrap();
    let mut build = BuildRecord::capture(&original).restore(content).unwrap();
    choose(&mut build, 0, "first_form").unwrap();
    build
        .select_talent(tor_simulation::HdIndex(1), child.clone())
        .unwrap();
    let replacement = AdvancementPurchase {
        pool: tor_simulation::content_ids::PointPoolId::new("advancement").unwrap(),
        selection: PurchaseSelection::TalentForm {
            talent: talent(),
            form: form("second_form"),
        },
    };
    let preview = build
        .preview_replacement(
            PurchaseLocation {
                owner: tor_simulation::HdIndex(0),
                index: 0,
            },
            Some(replacement),
        )
        .unwrap();
    assert_eq!(preview.removed().len(), 1);
    assert!(preview.derived().active_talents.contains(&child));
    build.commit_replacement(preview).unwrap();
    let refund = build
        .preview_replacement(
            PurchaseLocation {
                owner: tor_simulation::HdIndex(0),
                index: 0,
            },
            None,
        )
        .unwrap();
    assert_eq!(refund.removed().len(), 2);
    assert_eq!(refund.build().unspent_points(), 3);
    assert!(!refund.derived().active_talents.contains(&child));
    assert!(!refund.derived().abilities.contains(&ability("second_form")));
}
