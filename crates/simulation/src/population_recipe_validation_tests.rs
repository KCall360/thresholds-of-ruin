fn repeated_recipe_fixture() -> (Game, PopulationPlan) {
    let species = crate::test_creatures::species();
    let content = crate::test_creatures::content_with(species.clone(), vec![]);
    let build = crate::test_creatures::build(species);
    let game = Game::region_corridor(42, 2, content);
    let mut plan = carried_plan([1, 1]);
    let actor = &mut carrier_choice(&mut plan).actor;
    actor.build = Some(crate::creatures::record::BuildRecord::capture(&build));
    actor.identity = Some(crate::CreatureIdentity {
        name: "carrier".into(),
        faction: "neutral".into(),
    });
    let PopulationContent::Actors { bands, .. } =
        &mut plan.profiles.get_mut("baseline").unwrap().stages[0].population
    else {
        unreachable!()
    };
    let first = bands[0].clone();
    *bands = (1..=16)
        .map(|depth| {
            let mut band = first.clone();
            band.depth = DepthInterval::Finite {
                minimum: depth,
                maximum: depth,
            };
            band.choices[0].weight = depth;
            band
        })
        .collect();
    (game, plan)
}

fn repeated_recipe_choice(plan: &mut PopulationPlan, band: usize) -> &mut PreparedActorChoice {
    let PopulationContent::Actors { bands, .. } =
        &mut plan.profiles.get_mut("baseline").unwrap().stages[0].population
    else {
        unreachable!()
    };
    &mut bands[band].choices[0]
}

#[test]
fn population_recipe_validation_counts_complete_recipes_once_per_invocation() {
    let (game, plan) = repeated_recipe_fixture();
    let before = game.clone();
    for _ in 0..2 {
        let mut validation = PopulationRecipeValidation::new(&game);
        assert!(plan.valid_with_recipes(&mut validation));
        assert_eq!((validation.actor_checks, validation.birth_checks), (1, 1));
        assert_eq!(validation.successful.len(), 1);
    }
    assert_eq!(game, before);
}

#[test]
fn population_recipe_validation_rejects_same_label_changed_build_growth_or_schedule() {
    let (game, original) = repeated_recipe_fixture();
    for case in 0..5 {
        let mut plan = original.clone();
        let choice = repeated_recipe_choice(&mut plan, 1);
        match case {
            0 => {
                let mut build = serde_json::to_value(choice.actor.build.as_ref().unwrap()).unwrap();
                build["species"] = serde_json::json!("missing-species");
                choice.actor.build = Some(serde_json::from_value(build).unwrap());
            }
            1 => choice.depth_growth = DepthGrowth::RacialAfter { depth: 0 },
            2 => choice
                .advancement
                .push(crate::prepared_spawn::OrdinalAdvancement {
                    ordinal: crate::HdCount(1),
                    choices: Default::default(),
                }),
            3 => choice.actor.identity.as_mut().unwrap().name.clear(),
            4 => choice.actor.known_identities.push(String::new()),
            _ => unreachable!(),
        }
        let mut validation = PopulationRecipeValidation::new(&game);
        assert!(!plan.valid_with_recipes(&mut validation), "case {case}");
        assert_eq!(validation.actor_checks, 2, "case {case}");
        assert_eq!(validation.successful.len(), 1, "case {case}");
    }
}

#[test]
fn population_recipe_validation_keeps_every_weight_turn_and_carried_policy_check() {
    let (game, original) = repeated_recipe_fixture();
    for case in 0..5 {
        let mut plan = original.clone();
        let choice = repeated_recipe_choice(&mut plan, 1);
        match case {
            0 => choice.weight = 0,
            1 => choice.turn_ticks = 0,
            2 => choice.carried_loot.as_mut().unwrap().pool = "missing".into(),
            3 => choice.carried_loot.as_mut().unwrap().chance.denominator = 0,
            4 => choice.carried_loot.as_mut().unwrap().version = 2,
            _ => unreachable!(),
        }
        let before = game.clone();
        assert!(!plan.valid(&game), "case {case}");
        assert_eq!(game, before);
    }
}

#[test]
fn population_recipe_validation_never_caches_failure_or_trusts_a_label() {
    let (game, mut plan) = repeated_recipe_fixture();
    let original = carrier_choice(&mut plan).clone();
    let mut validation = PopulationRecipeValidation::new(&game);
    assert!(validation.valid(&original));
    let mut changed = original.clone();
    changed.actor.asset = Some(String::new());
    for expected in [2, 3] {
        assert!(!validation.valid(&changed));
        assert_eq!(validation.actor_checks, expected);
    }
    assert!(validation.valid(&original));
    assert_eq!(validation.actor_checks, 3);
    changed.actor.asset = Some("different.valid.asset".into());
    assert!(validation.valid(&changed));
    assert_eq!((validation.actor_checks, validation.birth_checks), (4, 2));
}

#[test]
fn population_recipe_validation_different_content_cannot_reuse_success() {
    let (source, plan) = repeated_recipe_fixture();
    assert!(plan.valid(&source));
    let different = game(); // This content has no carrier species.
    let mut validation = PopulationRecipeValidation::new(&different);
    assert!(!plan.valid_with_recipes(&mut validation));
    assert_eq!((validation.actor_checks, validation.birth_checks), (1, 0));
    assert!(validation.successful.is_empty());
}

#[test]
fn population_recipe_validation_memory_is_bounded_for_diverse_recipes() {
    let (game, mut plan) = repeated_recipe_fixture();
    let original = carrier_choice(&mut plan).clone();
    let mut validation = PopulationRecipeValidation::new(&game);
    for n in 0..=MAX_CHOICES {
        let mut choice = original.clone();
        choice.content = format!("recipe-{n}");
        assert!(validation.valid(&choice));
    }
    assert_eq!(validation.successful.len(), MAX_CHOICES);
    assert_eq!(validation.actor_checks, MAX_CHOICES + 1);
    let mut uncached = original.clone();
    uncached.content = format!("recipe-{MAX_CHOICES}");
    assert!(validation.valid(&uncached));
    assert_eq!(validation.actor_checks, MAX_CHOICES + 2);
}
