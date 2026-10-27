//! DiscernibleEffect is device-only and currently recognizes Polymorph only.
use super::*;
use crate::rules::{
    Against, AmountExpr, CheckActor, CheckDef, Effect, Interaction, Reach, SkillSelector,
    StrikeSource,
};
use crate::{
    ActorId, ChargeRange, ConsumableSpec, DeviceSpec, ItemDiscovery, ItemSpec, ReadableSpec, Work,
};

fn heal() -> Effect {
    Effect::Heal {
        amount: AmountExpr::Fixed { amount: 1 },
    }
}
fn device() -> DeviceSpec {
    DeviceSpec {
        ability: crate::content_ids::AbilityId::new("fixture_ray").unwrap(),
        initial_charges: ChargeRange {
            minimum: 1,
            maximum: 1,
        },
        discovery: ItemDiscovery::OnUse,
    }
}
fn ray() -> crate::Game {
    super::ray_tests::fixture_with(|_, _, _| {})
}

#[test]
fn discernible_policy_accepts_only_supported_polymorph_device_definitions() {
    let ordinary = ray();
    let mut spec = device();
    for policy in [ItemDiscovery::OnUse, ItemDiscovery::Never] {
        spec.discovery = policy;
        spec.validate_rules(ordinary.content().rules()).unwrap();
    }
    spec.discovery = ItemDiscovery::DiscernibleEffect;
    assert!(spec.validate_rules(ordinary.content().rules()).is_err());
    for bounded in [false, true] {
        let game = super::ray_tests::fixture_with(|_, definition, _| {
            definition.invocation = InvocationKind::DirectionalContactRay;
            definition.reach = if bounded {
                Reach::Sight { range: 4 }
            } else {
                Reach::UntilObstruction
            };
            let StrikeSource::Authored { strikes } = &mut definition.strikes else {
                unreachable!()
            };
            strikes[0].descriptors.clear();
            strikes[0].check = Some(CheckDef {
                actor: CheckActor::Target,
                skill: SkillSelector::Named {
                    skill: crate::attributes::Skill::Agility,
                },
                against: Against::Opposed {
                    skill: SkillSelector::Named {
                        skill: crate::attributes::Skill::Lore,
                    },
                },
            });
            strikes[0].on_success.clear();
            strikes[0].on_failure = vec![Effect::BalefulPolymorph {
                forms: vec![
                    crate::content_ids::BodyFormId::new("eligible_at_catalog_admission").unwrap(),
                ],
            }];
        });
        // Complete ordinary two-stage catalog construction before treating the
        // Polymorph definition as a positive admission control.
        let id = crate::content_ids::BodyFormId::new("eligible_at_catalog_admission").unwrap();
        let package = std::sync::Arc::new(crate::creatures::BodyPackage {
            id: id.clone(),
            species: game
                .creature(ActorId(1))
                .unwrap()
                .build()
                .species_id()
                .clone(),
            body: crate::BodySpec::default(),
            advancement: vec![crate::creatures::PlannedAdvancement {
                source: crate::progression::HdSource::Racial,
                choices: crate::creatures::HdChoices::default(),
            }],
        });
        let content = crate::creatures::Content::with_body_packages(
            game.content(),
            [(id.clone(), package)].into(),
        )
        .unwrap();
        let content =
            crate::creatures::Content::with_polymorph_forms(&content, [id.clone()].into()).unwrap();
        assert!(content.polymorph_body_package(&id).is_some());
        spec.validate_rules(content.rules()).unwrap();
    }
    let self_heal = super::ray_tests::fixture_with(|_, definition, _| {
        definition.invocation = InvocationKind::ActorTarget;
        definition.interaction = Interaction::NonHostile;
        definition.ai.hostile = false;
        definition.tags.clear();
        definition.reach = Reach::SelfOnly;
        let StrikeSource::Authored { strikes } = &mut definition.strikes else {
            unreachable!()
        };
        strikes[0].check = None;
        strikes[0].descriptors.clear();
        strikes[0].on_failure.clear();
        strikes[0].on_success = vec![heal()];
    });
    spec.discovery = ItemDiscovery::OnUse;
    spec.validate_rules(self_heal.content().rules()).unwrap();
    spec.discovery = ItemDiscovery::DiscernibleEffect;
    assert!(spec.validate_rules(self_heal.content().rules()).is_err());
}

#[test]
fn nondevice_payloads_keep_existing_policies_but_reject_discernible_policy() {
    for policy in [
        ItemDiscovery::SelfEffect,
        ItemDiscovery::OnUse,
        ItemDiscovery::Never,
    ] {
        let mut consumable = ConsumableSpec {
            descriptors: vec![],
            effects: vec![heal()],
            discovery: policy,
        };
        assert!(consumable.valid());
        consumable.discovery = ItemDiscovery::DiscernibleEffect;
        assert!(!consumable.valid());
        for control in [None, Some(crate::control_scrolls::ControlSpec::Nothing {})] {
            let mut readable = ReadableSpec {
                effects: if control.is_some() {
                    vec![]
                } else {
                    vec![heal()]
                },
                control,
                descriptors: vec![],
                identify: None,
                discovery: policy,
            };
            assert!(readable.valid());
            readable.discovery = ItemDiscovery::DiscernibleEffect;
            assert!(!readable.valid());
        }
    }
}

#[test]
fn frozen_item_plans_cannot_smuggle_unsupported_discernible_policy() {
    let actor = ActorId(1);
    for kind in 0..4 {
        let mut game = ray();
        let mut spec = ItemSpec::ordinary("policy witness".into());
        spec.class = match kind {
            0 => crate::ItemClass::Wand,
            1 => crate::ItemClass::Potion,
            _ => crate::ItemClass::Scroll,
        };
        match kind {
            0 => spec.device = Some(device()),
            1 => {
                spec.consumable = Some(ConsumableSpec {
                    descriptors: vec![],
                    effects: vec![heal()],
                    discovery: ItemDiscovery::SelfEffect,
                })
            }
            _ => {
                spec.readable = Some(ReadableSpec {
                    control: (kind == 3).then_some(crate::control_scrolls::ControlSpec::Nothing {}),
                    descriptors: vec![],
                    effects: if kind == 3 { vec![] } else { vec![heal()] },
                    identify: None,
                    discovery: ItemDiscovery::SelfEffect,
                })
            }
        }
        assert!(spec.valid(), "valid positive item fixture for kind {kind}");
        let item = game
            .place_item_stack(
                game.next_item_id,
                game.actors[&actor].location,
                Some(actor),
                1,
                spec,
            )
            .unwrap();
        let work = match kind {
            0 => Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: crate::abilities::StepDirection::East,
                },
            },
            1 => Work::Drink { item },
            _ => Work::Read { item },
        };
        let mut plan = game.plan_work(actor, work).unwrap();
        assert!(plan.valid(game.content().rules()));
        match &mut plan {
            Plan::Zap { discovery, .. }
            | Plan::Read { discovery, .. }
            | Plan::Drink { discovery, .. } => *discovery = ItemDiscovery::DiscernibleEffect,
            _ => unreachable!(),
        }
        assert!(
            !plan.valid(game.content().rules()),
            "unsupported frozen policy for kind {kind}"
        );
    }
}
