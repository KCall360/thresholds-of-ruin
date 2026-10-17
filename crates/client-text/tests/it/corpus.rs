//! A corpus of phrasings players type, each with the meaning the game should
//! take from it, read through the same pipeline as play: idioms and
//! modifiers, the disclosed grammar, then verbs against the scene.
//!
//! Adding a phrasing here is how a new way of saying something is pinned.
use tor_client_common::Palette;
use tor_client_text::{
    engine::{
        resolve::Referents,
        scene::Scene,
        verbs::{interpret, Goal, Interpretation},
    },
    parser::{idiom, match_disclosed_sentence, tokenize},
};
use tor_protocol::*;

fn state() -> StateView {
    let mut abilities = vec![tor_client_common::test_support::ability("basic_melee")];
    let mut rattle = tor_client_common::test_support::ability("rattling_blow");
    rattle.name = "Rattling Blow".into();
    abilities.push(rattle);
    let mut brace = tor_client_common::test_support::ability("brace_for_impact");
    brace.name = "Brace for Impact".into();
    brace.reach = AbilityReach::SelfOnly;
    brace.target_kind = AbilityTargetKind::SelfOnly;
    abilities.push(brace);
    let mut stats = tor_client_common::test_support::stats();
    stats.abilities = abilities;
    serde_json::from_value(serde_json::json!({
        "wizard_game":false,"revision":"0","observation":{
        "actor":"1","self_target":super::actor_target(1),"tick":"0","position":{"x":0,"y":0,"z":0},"ready":true,
        "places":[],"visible_cells":(0..7).map(|x| serde_json::json!({
            "key":format!("cell-{x}"),"position":{"x":x,"y":0,"z":0},
            "wall":false,"material":"stone","place_hint":x==1,
            "stairs_up":false,"stairs_down":false
        })).collect::<Vec<_>>(),
        "ground_items":[
            {"supply_reachable":false,"reachable":true,"item":{"known_supply":null,"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(1),"name":"copper token","description":"A small copper disc."},"position":{"x":0,"y":0,"z":0}},
            {"supply_reachable":false,"reachable":false,"item":{"known_supply":null,"quantity":"1","class":"food","appearance":"item","identified":true,"id":super::item_target(2),"name":"food ration","description":""},"position":{"x":1,"y":0,"z":0}},
            {"supply_reachable":false,"reachable":true,"item":{"known_supply":null,"quantity":"1","class":"corpse","appearance":"item","identified":true,"id":super::item_target(3),"name":"ruin scout corpse","description":""},"position":{"x":0,"y":0,"z":0}}
        ],
        "inventory":[
            {"known_supply":null,"quantity":"1","class":"weapon","appearance":"item","identified":true,"id":super::item_target(4),"name":"iron greatsword","description":""}
        ],
        "linked_holds":[],"visible_actors":[
            {"id":super::actor_target(2),"name":"goblin","description":"","position":{"x":3,"y":0,"z":0}}
        ],
        "combat":{"own_stats":stats,"public_plan":null,"hp":"20","max_hp":"20","sequence":null,"preparation_remaining":null,"preparation_active":false,
            "recovery_remaining":"0","actors":[{"actor":super::actor_target(2),"hostile":true,"allied":false,"injury":"healthy"}],
            "events":[],"objective":"retrieve_and_return","exit":"cell-0",
            "victory":false,"dead":false,"terminal":false}}}))
    .unwrap()
}

/// What a phrasing should mean.
#[derive(Debug)]
enum Means {
    /// An attack on the goblin, approaching first if need be.
    Attack,
    /// Fighting the goblin on.
    Fight,
    Follow,
    Back,
    Flee,
    /// Picking up these items, in order.
    Take(&'static [u64]),
    /// Using this ability.
    Ability(&'static str),
    /// A question asking for what's missing.
    Asks(&'static str),
    /// An answer containing these words.
    Says(&'static str),
    /// A refusal containing these words.
    Refuses(&'static str),
    Looks,
    Waits,
    /// Walking a way.
    Goes,
}

fn read(line: &str, s: &StateView) -> Interpretation {
    let (text, modifiers) = idiom::normalize(line);
    let tokens = tokenize(&text);
    let mut command = match_disclosed_sentence(&tokens, &text, &s.observation)
        .unwrap_or_else(|e| panic!("{line}: {e}"));
    // As the engine does for "until it dies".
    if modifiers.persist {
        if let tor_client_text::parser::ParsedCommand::Transitive { verb, .. } = &mut command {
            if *verb == tor_client_text::parser::Verb::Attack {
                *verb = tor_client_text::parser::Verb::Fight;
            }
        }
    }
    let palette = Palette::default();
    let scene = Scene::new(s, &palette);
    interpret(&command, &scene, &mut Referents::default())
}

fn goblin() -> ActorTarget {
    super::actor_target(2)
}

fn fits(meant: &Interpretation, means: &Means) -> bool {
    let goals = |i: &Interpretation| -> Vec<Goal> {
        match i {
            Interpretation::Goals(g) => g.clone(),
            Interpretation::Sequence(parts) => parts
                .iter()
                .flat_map(|p| match p {
                    Interpretation::Goals(g) => g.clone(),
                    _ => Vec::new(),
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    let said = |i: &Interpretation| match i {
        Interpretation::Say(t) => Some(t.clone()),
        Interpretation::Sequence(parts) => parts.iter().find_map(|p| match p {
            Interpretation::Say(t) => Some(t.clone()),
            _ => None,
        }),
        _ => None,
    };
    match means {
        Means::Attack => goals(meant) == [Goal::Attack { target: goblin() }],
        Means::Fight => match meant {
            Interpretation::Fight(t) => *t == goblin(),
            Interpretation::Sequence(parts) => parts
                .iter()
                .any(|p| matches!(p, Interpretation::Fight(t) if *t == goblin())),
            _ => false,
        },
        Means::Follow => match meant {
            Interpretation::Follow(t) => *t == goblin(),
            Interpretation::Sequence(parts) => parts
                .iter()
                .any(|p| matches!(p, Interpretation::Follow(t) if *t == goblin())),
            _ => false,
        },
        Means::Back => matches!(meant, Interpretation::Back { flee: false }),
        Means::Flee => matches!(meant, Interpretation::Back { flee: true }),
        Means::Take(items) => {
            goals(meant)
                == items
                    .iter()
                    .map(|i| Goal::Take {
                        item: super::item_target(*i),
                        quantity: None,
                    })
                    .collect::<Vec<_>>()
        }
        Means::Ability(id) => goals(meant)
            .iter()
            .any(|g| matches!(g, Goal::UseAbility { ability, .. } if ability == id)),
        Means::Asks(prompt) => {
            matches!(meant, Interpretation::Orphan { prompt: p, .. } if p == prompt)
        }
        Means::Says(words) => said(meant).is_some_and(|t| t.contains(words)),
        Means::Refuses(words) => {
            matches!(meant, Interpretation::Refuse(t) if t.contains(words))
        }
        Means::Looks => matches!(meant, Interpretation::Look),
        Means::Waits => goals(meant) == [Goal::Wait],
        Means::Goes => goals(meant).iter().any(|g| {
            matches!(
                g,
                Goal::Go { .. } | Goal::Visit { .. } | Goal::Approach { .. }
            )
        }),
    }
}

const CORPUS: &[(&str, Means)] = &[
    // Fighting.
    ("attack goblin", Means::Attack),
    ("attack the goblin", Means::Attack),
    ("kill the goblin", Means::Attack),
    ("hit him", Means::Attack),
    ("attack", Means::Attack),
    ("kill", Means::Attack),
    ("attack the goblin again", Means::Attack),
    ("attack the goblin carefully", Means::Attack),
    ("please attack the goblin", Means::Attack),
    ("I want to attack the goblin", Means::Attack),
    ("swing at the goblin", Means::Attack),
    ("swing my sword at the goblin", Means::Attack),
    ("charge the goblin", Means::Attack),
    ("strike the goblin", Means::Attack),
    ("slash goblin", Means::Attack),
    ("attack the goblin with my sword", Means::Attack),
    ("attack the creature", Means::Attack),
    ("attack the monster", Means::Attack),
    ("attack the enemy", Means::Attack),
    ("go for the goblin", Means::Attack),
    ("fight the goblin", Means::Fight),
    ("fight", Means::Fight),
    ("keep attacking", Means::Fight),
    ("keep attacking the goblin", Means::Fight),
    ("attack the goblin until it dies", Means::Fight),
    ("attack goblin to the death", Means::Fight),
    ("finish off the goblin", Means::Fight),
    ("follow the goblin", Means::Follow),
    ("chase the goblin", Means::Follow),
    ("go after the goblin", Means::Follow),
    ("flee", Means::Flee),
    ("run away", Means::Flee),
    ("retreat", Means::Flee),
    ("back", Means::Back),
    ("go back", Means::Back),
    ("go back the way i came", Means::Back),
    // Techniques.
    ("rattling blow goblin", Means::Ability("rattling_blow")),
    ("rattling goblin", Means::Ability("rattling_blow")),
    (
        "rattling blow",
        Means::Asks("Who do you want to use Rattling Blow on?"),
    ),
    ("brace for impact", Means::Ability("brace_for_impact")),
    ("brace", Means::Ability("brace_for_impact")),
    (
        "measured cut goblin",
        Means::Refuses("You haven't learned Measured Cut"),
    ),
    (
        "spark lance fire goblin",
        Means::Refuses("You haven't learned Spark Lance"),
    ),
    // Taking.
    ("take the token", Means::Take(&[1])),
    ("get token", Means::Take(&[1])),
    ("pick up the token", Means::Take(&[1])),
    ("pick the token up", Means::Take(&[1])),
    ("take all", Means::Take(&[1, 2])),
    ("take everything", Means::Take(&[1, 2])),
    ("take the body", Means::Take(&[3])),
    ("take the scout's body", Means::Take(&[3])),
    ("take all corpses", Means::Take(&[3])),
    ("grab the food", Means::Take(&[2])),
    ("take", Means::Asks("What do you want to take?")),
    // Using things.
    ("eat the ration", Means::Take(&[2])),
    ("drink", Means::Asks("What do you want to drink?")),
    (
        "bind wounds",
        Means::Refuses("You haven't learned Bind Wounds"),
    ),
    // Looking and asking.
    ("look", Means::Looks),
    ("l", Means::Looks),
    ("look around", Means::Looks),
    ("examine the goblin", Means::Says("goblin")),
    ("x goblin", Means::Says("It means you harm.")),
    ("what is the goblin", Means::Says("It means you harm.")),
    (
        "look at my sword",
        Means::Says("The iron greatsword is a weapon."),
    ),
    (
        "examine the weapon",
        Means::Says("The iron greatsword is a weapon."),
    ),
    (
        "x greatsword",
        Means::Says("The iron greatsword is a weapon."),
    ),
    (
        "examine the corpse",
        Means::Says("all that's left of the ruin scout"),
    ),
    (
        "examine the exit",
        Means::Says("You are standing at the exit"),
    ),
    ("look east", Means::Says("To the east you see")),
    ("l e", Means::Says("To the east you see")),
    ("exits", Means::Says("Ways on:")),
    ("where can i go", Means::Says("Ways on:")),
    ("where am i", Means::Says("You are in")),
    ("what can i do", Means::Says("You could")),
    ("what now", Means::Says("You could")),
    ("who am i", Means::Says("stats shows your full build")),
    (
        "inventory",
        Means::Says("You are carrying an iron greatsword"),
    ),
    (
        "what am i carrying",
        Means::Says("You are carrying an iron greatsword"),
    ),
    (
        "take inventory",
        Means::Says("You are carrying an iron greatsword"),
    ),
    ("how am i", Means::Says("HP 20/20")),
    ("am i hurt", Means::Says("HP 20/20")),
    ("score", Means::Says("Find what you came for")),
    // The game itself.
    ("undo", Means::Says("What's done is done")),
    ("restart", Means::Says("To start over")),
    ("time", Means::Says("no telling the hour")),
    ("options", Means::Says("")),
    ("yes", Means::Says("There's no question to answer.")),
    ("xyzzy", Means::Says("A hollow voice")),
    ("dance", Means::Says("The stones are unimpressed")),
    ("sing", Means::Says("The stone swallows the sound")),
    ("jump", Means::Refuses("You jump on the spot")),
    ("sleep", Means::Refuses("Use wait to rest")),
    ("talk to the goblin", Means::Refuses("inclined to talk")),
    (
        "search",
        Means::Refuses("find nothing you hadn't already seen"),
    ),
    ("search the corpse", Means::Says("You find nothing hidden")),
    // Moving.
    ("wait", Means::Waits),
    ("z", Means::Waits),
    ("go to the goblin", Means::Goes),
    ("walk toward the goblin", Means::Goes),
    ("go to the exit", Means::Says("already at the exit")),
    // Not understood, with help.
    ("atack goblin", Means::Refuses("Did you mean \"attack\"?")),
    (
        "frobnicate the goblin",
        Means::Refuses("I don't know the word \"frobnicate\""),
    ),
    (
        "examine the goblin's lantern",
        Means::Refuses("I only understood you as far as wanting to examine the goblin"),
    ),
];

#[test]
fn the_corpus_reads_as_meant() {
    let s = state();
    let mut misread = Vec::new();
    for (line, means) in CORPUS {
        let meant = read(line, &s);
        // "options" needs the engine; the reading is all that's checked.
        let fine = if *line == "options" {
            matches!(meant, Interpretation::Options)
        } else {
            fits(&meant, means)
        };
        if !fine {
            misread.push(format!("{line:?}: expected {means:?}, got {meant:?}"));
        }
    }
    assert!(
        misread.is_empty(),
        "{} of {} phrasings misread:\n{}",
        misread.len(),
        CORPUS.len(),
        misread.join("\n")
    );
}
