use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::{
    ai::AiProfile, combat::CombatSpec, diagnostics::work_counts, Action, ActorId, Game,
};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

#[test]
fn a_many_target_decision_uses_one_remembered_topology_search() {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_region(Region {
            id: RegionId(1),
            name: "arena".into(),
            bounds: Extent::new(16, 16, 1).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42);
    let at = |x, y| Location {
        region: RegionId(1),
        position: Position { x, y, z: 0 },
    };
    let turn = NonZeroU64::new(100).unwrap();
    let human = game.spawn_actor(at(7, 7), turn).unwrap();
    let ai = game.spawn_actor(at(7, 8), turn).unwrap();
    for x in 5..9 {
        for y in 5..9 {
            if (x, y) != (7, 7) && (x, y) != (7, 8) {
                game.spawn_actor(at(x, y), turn).unwrap();
            }
        }
    }
    for id in 1..=16 {
        game.configure_combat(
            ActorId(id),
            CombatSpec {
                faction: if ActorId(id) == ai { "foe" } else { "hero" }.into(),
                ..Default::default()
            },
        )
        .unwrap();
    }
    game.configure_run(
        human,
        BTreeSet::from([human]),
        None,
        BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
    )
    .unwrap();
    game.configure_ai(ai, AiProfile::default()).unwrap();
    game.refresh_navigation();
    game.act(human, Action::Wait).unwrap();
    assert_eq!(game.observe(ai).unwrap().visible_actors.len(), 15);
    let before = work_counts().route_searches;
    assert_eq!(
        game.next_ai_action(),
        Some((ai, Action::Attack { target: human }))
    );
    assert_eq!(work_counts().route_searches - before, 1);
}
