//! Packages with one file per region, read only as regions are built, and
//! saves that pin their package and copy each region file they build from.
//! See docs/scenario-packages.md.
use crate::support;
use std::path::Path;
use tor_protocol::{Action, ActorId, Direction};
use tor_server::{journal::Command, scenario_package, Engine, SavePolicy, Scenario, Streaming};

fn corridor_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor")
}

/// The streaming corridor with `halls` halls, validated at `out`.
fn corridor(out: &Path, halls: u64) -> Scenario {
    scenario_package::streaming_corridor(&corridor_root(), out, halls, 5).unwrap();
    scenario_package::validate(out).unwrap();
    scenario_package::load(out, 5, None, false).unwrap()
}

/// A copy of the checked-in seven-hall corridor at `out`.
fn seven_halls(out: &Path) -> Scenario {
    support::copy_package(&corridor_root(), out);
    scenario_package::load(out, 5, None, false).unwrap()
}

/// The seven halls with every hall loaded from the start, so all are built
/// without walking past the guard.
fn seven_halls_built(out: &Path) -> Scenario {
    let mut scenario = seven_halls(out);
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 6,
    });
    scenario
}

fn act(engine: &mut Engine, action: Action) -> Result<(), tor_server::Failure> {
    let revision = engine.revision(ActorId(1)).unwrap();
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            &uuid::Uuid::new_v4().to_string(),
            &engine.branch().clone(),
            Command::Act {
                expected_revision: revision,
                action,
            },
        )
        .map(|_| ())
}

fn walk(engine: &mut Engine, direction: Direction, steps: usize) {
    for _ in 0..steps {
        act(engine, Action::Move { direction }).unwrap();
    }
}

/// At the default radii, 20 steps east reach hall 2, which builds hall 4
/// (reading hall 5's walls); 20 more reach hall 3, which builds hall 5.
const TO_HALL_2: usize = 20;

fn copies(save: &Path) -> Vec<i64> {
    let db = rusqlite::Connection::open(save).unwrap();
    let mut query = db
        .prepare("SELECT region FROM region_sources ORDER BY region")
        .unwrap();
    query
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn replay_everything() -> SavePolicy {
    SavePolicy {
        checkpoint_interval: 0,
        ..SavePolicy::default()
    }
}

#[test]
fn format_1_packages_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    support::copy_package(&root, temp.path());
    let manifest = temp.path().join("scenario.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, text.replace("format = 2", "format = 1")).unwrap();
    let error = scenario_package::load(temp.path(), 42, None, true).unwrap_err();
    assert!(error.message.contains("format"), "{error}");
}

#[test]
fn region_files_are_named_by_their_region_and_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    support::copy_package(&root, temp.path());
    let regions = temp.path().join("regions");
    std::fs::rename(regions.join("2.toml"), regions.join("3.toml")).unwrap();
    let error = scenario_package::validate(temp.path()).unwrap_err();
    assert!(error.message.contains("regions/2.toml"), "{error}");
    std::fs::rename(regions.join("3.toml"), regions.join("hall.toml")).unwrap();
    let error = scenario_package::validate(temp.path()).unwrap_err();
    assert!(error.message.contains("<region id>.toml"), "{error}");
    std::fs::rename(regions.join("hall.toml"), regions.join("2.toml")).unwrap();
    let mut text = std::fs::read_to_string(regions.join("2.toml")).unwrap();
    text.push_str(&format!("# {}\n", "x".repeat(1024 * 1024)));
    std::fs::write(regions.join("2.toml"), text).unwrap();
    let error = scenario_package::validate(temp.path()).unwrap_err();
    assert!(error.message.contains("exceeds"), "{error}");
}

#[test]
fn a_carried_item_is_authored_where_its_carrier_starts() {
    let temp = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    support::copy_package(&root, temp.path());
    // The only character starts in region 1; the tablet is in region 2.
    support::edit_region(
        temp.path(),
        "\"archetype\" = \"tablet\"",
        "\"archetype\" = \"tablet\", carried_by = 1",
    );
    let error = scenario_package::validate(temp.path()).unwrap_err();
    assert!(
        error.message.contains("where its carrier starts"),
        "{error}"
    );
}

#[test]
fn starting_a_validated_package_reads_only_the_region_files_it_builds() {
    let temp = tempfile::tempdir().unwrap();
    let read = |halls: u64| {
        let scenario = corridor(&temp.path().join(format!("c{halls}")), halls);
        let package = scenario.package.clone().unwrap();
        Engine::memory(scenario).unwrap();
        package.sources.files_read()
    };
    let small = read(16);
    assert_eq!(small, read(256));
    // The start region, its loaded neighbours, and theirs for walls.
    assert!((1..=4).contains(&small), "{small}");
}

#[test]
fn a_save_copies_only_the_region_files_its_builds_read() {
    let temp = tempfile::tempdir().unwrap();
    let scenario = corridor(&temp.path().join("package"), 256);
    let save = temp.path().join("game.db");
    let mut engine = Engine::open(&save, scenario).unwrap();
    walk(&mut engine, Direction::East, 68);
    let built = {
        let c = engine.region_counts().unwrap();
        c.active + c.frozen + c.detached
    };
    engine.flush().unwrap();
    drop(engine);
    let copied = copies(&save);
    // Each built hall, and the next one, whose walls its build read.
    assert_eq!(copied.len(), built + 1, "{copied:?}");
    assert_eq!(copied, (1..=built as i64 + 1).collect::<Vec<_>>());
}

#[test]
fn resuming_reads_uncopied_region_files_from_the_package_directory() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("package");
    let save = temp.path().join("game.db");
    let mut engine = Engine::open(&save, seven_halls(&package)).unwrap();
    walk(&mut engine, Direction::East, TO_HALL_2);
    engine.flush().unwrap();
    drop(engine);
    assert_eq!(copies(&save), (1..=5).collect::<Vec<_>>());
    // No --scenario: the save remembers where its package was.
    let mut engine = Engine::open(&save, Scenario::two_room(0)).unwrap();
    assert_eq!(engine.region_counts().unwrap().unbuilt, 3);
    walk(&mut engine, Direction::East, TO_HALL_2);
    assert_eq!(engine.region_counts().unwrap().unbuilt, 2);
    engine.flush().unwrap();
    drop(engine);
    assert_eq!(copies(&save), (1..=6).collect::<Vec<_>>());
}

#[test]
fn resuming_without_the_package_is_refused_while_regions_are_unbuilt_unless_it_is_named() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("package");
    let save = temp.path().join("game.db");
    let mut engine = Engine::open(&save, seven_halls(&package)).unwrap();
    walk(&mut engine, Direction::East, TO_HALL_2);
    engine.flush().unwrap();
    let expected = engine.state(ActorId(1)).unwrap();
    drop(engine);
    let moved = temp.path().join("moved");
    std::fs::rename(&package, &moved).unwrap();
    let before = std::fs::read(&save).unwrap();
    let error = Engine::open(&save, Scenario::two_room(0)).unwrap_err();
    assert!(error.message.contains("isn't available"), "{error}");
    assert!(error.message.contains("--scenario"), "{error}");
    assert_eq!(std::fs::read(&save).unwrap(), before);
    // Naming the moved package resumes, and play reaches the rest.
    let named = scenario_package::load(&moved, 0, None, false).unwrap();
    let mut engine = Engine::open(&save, named).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected);
    walk(&mut engine, Direction::East, TO_HALL_2);
    assert_eq!(engine.region_counts().unwrap().unbuilt, 2);
}

#[test]
fn a_changed_package_is_refused_on_resume() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("package");
    let save = temp.path().join("game.db");
    let mut engine = Engine::open(&save, seven_halls(&package)).unwrap();
    walk(&mut engine, Direction::East, TO_HALL_2);
    engine.flush().unwrap();
    drop(engine);
    // Hall 7 isn't built yet; its file changes and the package is revalidated.
    support::edit_region(&package, "name = \"Hall 7\"", "name = \"The last hall\"");
    scenario_package::validate(&package).unwrap();
    let error = Engine::open(&save, Scenario::two_room(0)).unwrap_err();
    assert!(error.message.contains("isn't available"), "{error}");
    let named = scenario_package::load(&package, 0, None, false).unwrap();
    assert!(Engine::open(&save, named).is_err());
}

#[test]
fn a_region_file_edited_without_revalidating_is_refused_when_built() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("package");
    let save = temp.path().join("game.db");
    let mut engine = Engine::open(&save, seven_halls(&package)).unwrap();
    // Building hall 4, on reaching hall 2, reads hall 5's walls.
    support::edit_region(&package, "name = \"Hall 5\"", "name = \"Changed\"");
    let mut refused = None;
    for _ in 0..TO_HALL_2 {
        let before = engine.state(ActorId(1)).unwrap();
        let counts = engine.region_counts().unwrap();
        if let Err(error) = act(
            &mut engine,
            Action::Move {
                direction: Direction::East,
            },
        ) {
            // The refused command changed nothing.
            assert_eq!(engine.state(ActorId(1)).unwrap(), before);
            assert_eq!(engine.region_counts().unwrap(), counts);
            refused = Some(error);
            break;
        }
    }
    let error = refused.expect("building hall 4 is refused");
    assert!(
        error.message.contains("regions/5.toml changed since"),
        "{error}"
    );
}

#[test]
fn a_fully_built_game_replays_from_its_own_copies_without_the_package() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("package");
    let save = temp.path().join("game.db");
    let mut engine =
        Engine::open_with_policy(&save, seven_halls_built(&package), replay_everything()).unwrap();
    assert_eq!(engine.region_counts().unwrap().unbuilt, 0);
    walk(&mut engine, Direction::East, 4);
    engine.flush().unwrap();
    let expected = engine.state(ActorId(1)).unwrap();
    drop(engine);
    assert_eq!(copies(&save), (1..=7).collect::<Vec<_>>());
    std::fs::remove_dir_all(&package).unwrap();
    let engine =
        Engine::open_with_policy(&save, Scenario::two_room(0), replay_everything()).unwrap();
    assert_eq!(engine.recovery_profile().records_replayed, 4);
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected);
}

#[test]
fn a_region_nothing_links_to_doesnt_block_resuming_without_the_package() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("package");
    support::copy_package(&corridor_root(), &package);
    // An eighth hall that no portal reaches: no build can ever need it.
    std::fs::write(
        package.join("regions/8.toml"),
        "id = 8\nname = \"Sealed hall\"\nsize = [4, 3, 1]\nanchors = { start = [1, 1, 0] }\n",
    )
    .unwrap();
    scenario_package::validate(&package).unwrap();
    let mut scenario = scenario_package::load(&package, 5, None, false).unwrap();
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 6,
    });
    let save = temp.path().join("game.db");
    let mut engine = Engine::open_with_policy(&save, scenario, replay_everything()).unwrap();
    walk(&mut engine, Direction::East, 4);
    engine.flush().unwrap();
    let expected = engine.state(ActorId(1)).unwrap();
    drop(engine);
    std::fs::remove_dir_all(&package).unwrap();
    let engine =
        Engine::open_with_policy(&save, Scenario::two_room(0), replay_everything()).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected);
}

#[test]
fn a_damaged_region_file_copy_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("package");
    let save = temp.path().join("game.db");
    let mut engine =
        Engine::open_with_policy(&save, seven_halls_built(&package), replay_everything()).unwrap();
    walk(&mut engine, Direction::East, 4);
    engine.flush().unwrap();
    drop(engine);
    std::fs::remove_dir_all(&package).unwrap();
    rusqlite::Connection::open(&save)
        .unwrap()
        .execute(
            "UPDATE region_sources SET source = source || '# damaged' WHERE region = 1",
            [],
        )
        .unwrap();
    let before = std::fs::read(&save).unwrap();
    // Replay rebuilds hall 1 from its copy, which no longer matches.
    let error =
        Engine::open_with_policy(&save, Scenario::two_room(0), replay_everything()).unwrap_err();
    assert!(error.message.contains("changed since"), "{error}");
    assert_eq!(std::fs::read(&save).unwrap(), before);
}
