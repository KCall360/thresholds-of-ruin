use std::{
    collections::BTreeSet,
    sync::{Arc, OnceLock},
};
use tor_world::{Direction, Location, Position, Region, RegionId};

use crate::{ActorId, Game, GameError, ItemId, ItemLocation};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SceneWitness {
    world: tor_world::PerceptionSnapshot,
    emitters: crate::portable_light::EmitterSignature,
    actor: ActorSceneWitness,
}

/// Actor-local part of the shared scene dependency proof. No emitter copies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActorSceneWitness {
    location: Location,
    orientation: u8,
    body: tor_world::Shared<crate::BodySpec>,
    disclosed: bool,
    sight: bool,
    nonvisual_range: u8,
}

/// Derived actor scenes for observations and streaming pins. Shared across
/// candidate clones, bounded, omitted from saves and ignored by simulation equality.
/// Rich disclosure and authority are always current. Actor classifications may
/// share immutable storage only under the exact visibility epoch and scene proof.
#[derive(Debug)]
pub(crate) struct CachedScene {
    witness: SceneWitness,
    cells: Arc<Vec<tor_world::SightCell>>,
    sight: Arc<Vec<tor_world::SightCell>>,
    // Preserve independent laziness/accounting for supply-membership queries.
    locations: OnceLock<BTreeSet<Location>>,
    projection: OnceLock<ProjectionIndexes>,
    hearing: OnceLock<BTreeSet<Location>>,
    streaming: OnceLock<(BTreeSet<RegionId>, Vec<RegionId>)>,
    contact: std::sync::Mutex<Option<SourceContactGeometry>>,
    classifications: std::sync::Mutex<Option<ClassifiedActors>>,
    raw_world: std::sync::Mutex<Option<Arc<RawWorldFacts>>>,
}

/// Geometric endpoints only: current combat/life and target body remain live.
#[derive(Debug)]
struct SourceContactGeometry {
    world: tor_world::SharedIdentity<tor_world::World>,
    locations: Arc<BTreeSet<Location>>,
}

/// Only canonical actor-reference classifications and resolved bodies, never
/// health, hostility, events, inventory, cosmetic views or mutable actors.
#[derive(Debug)]
struct ClassifiedActors {
    visibility: u64,
    projection: Arc<crate::perception::PerceptionProjection>,
}

#[derive(Debug)]
struct ProjectionIndexes {
    cells: BTreeSet<Location>,
    eyes: BTreeSet<Location>,
    occurrences: BTreeSet<(Location, u8)>,
}

impl CachedScene {
    fn new(
        witness: SceneWitness,
        cells: Vec<tor_world::SightCell>,
        sight: Vec<tor_world::SightCell>,
    ) -> Self {
        Self {
            witness,
            cells: Arc::new(cells),
            sight: Arc::new(sight),
            locations: OnceLock::new(),
            projection: OnceLock::new(),
            hearing: OnceLock::new(),
            streaming: OnceLock::new(),
            contact: std::sync::Mutex::new(None),
            classifications: std::sync::Mutex::new(None),
            raw_world: std::sync::Mutex::new(None),
        }
    }

    /// The caller checks living combat presence before consulting geometry.
    /// Use the canonical physical endpoint enumeration, not a target decision
    /// or selected hit witness. Arbitrary scenes and placements never enter here.
    pub(crate) fn contact_locations(
        &self,
        game: &Game,
        source: ActorId,
    ) -> Arc<BTreeSet<Location>> {
        if let Some(locations) = self
            .contact
            .lock()
            .expect("contact geometry")
            .as_ref()
            .filter(|entry| entry.world.matches(&game.world))
            .map(|entry| entry.locations.clone())
        {
            return locations;
        }
        // Physical contact may depend on World facts narrower perception does
        // not track. Nonowning identity prevents retaining entire old Worlds;
        // one payload per bounded scene replaces, rather than accumulates, versions.
        let locations = Arc::new(game.melee_contact_locations_in_scene(source, &self.cells, None));
        *self.contact.lock().expect("contact geometry") = Some(SourceContactGeometry {
            world: game.world.weak_identity(),
            locations: locations.clone(),
        });
        locations
    }

    pub(crate) fn classified_actors(
        &self,
        game: &Game,
        observer: ActorId,
    ) -> Result<Arc<crate::perception::PerceptionProjection>, GameError> {
        let visibility = game.actors.visibility_epoch();
        let cached = self
            .classifications
            .lock()
            .expect("actor classifications")
            .as_ref()
            .filter(|facts| facts.visibility == visibility)
            .map(|facts| facts.projection.clone());
        if let Some(cached) = cached {
            return Ok(cached);
        }
        // This scene proves world, emitters, observer body/chart and senses.
        // VisibilityInputs tracks every classify_body input, including actor
        // presence/life, body, invisibility, sight and guise recognition. It
        // intentionally invalidates more broadly than these reference facts.
        // Do not reuse this component for arbitrary/placement scenes.
        let indexes = self.projection();
        let projection = Arc::new(game.perception_projection_in_cells(
            observer,
            &self.cells,
            &indexes.cells,
            &indexes.eyes,
            None,
        )?);
        *self.classifications.lock().expect("actor classifications") = Some(ClassifiedActors {
            visibility,
            projection: projection.clone(),
        });
        Ok(projection)
    }

    fn raw_world_facts(&self, game: &Game) -> Arc<RawWorldFacts> {
        let cached = self
            .raw_world
            .lock()
            .expect("raw World facts")
            .as_ref()
            .filter(|facts| facts.world.matches(&game.world))
            .cloned();
        if let Some(cached) = cached {
            return cached;
        }
        // The scene payload already proves exact geometry and emitter inputs.
        // Its raw facts need the complete World identity as well: hints and
        // concealed metadata need not invalidate the narrower scene witness.
        // Never hold a cache lock while building facts or projecting disclosure.
        let facts = Arc::new(RawWorldFacts::capture(game, self));
        *self.raw_world.lock().expect("raw World facts") = Some(facts.clone());
        facts
    }

    fn projection(&self) -> &ProjectionIndexes {
        self.projection.get_or_init(|| ProjectionIndexes {
            cells: crate::perception::projection_locations(&self.cells),
            eyes: crate::perception::projection_locations(&self.sight),
            occurrences: self
                .cells
                .iter()
                .map(|cell| (cell.location, cell.rotation))
                .collect(),
        })
    }

    pub(crate) fn projection_cells(&self) -> &BTreeSet<Location> {
        &self.projection().cells
    }

    pub(crate) fn shared_cells(&self) -> Arc<Vec<tor_world::SightCell>> {
        self.cells.clone()
    }

    fn hearing_locations(&self, game: &Game) -> &BTreeSet<Location> {
        self.hearing.get_or_init(|| {
            let actor = &self.witness.actor;
            // Sound uses the reference position and frame, not the body's eye.
            let scene = game.world.eye_scene(actor.location, actor.orientation, 6);
            crate::diagnostics::hearing_scene(scene.len());
            scene
                .into_iter()
                .filter(|cell| cell.physical)
                .map(|cell| cell.location)
                .collect()
        })
    }

    fn contains(&self, at: Location) -> bool {
        self.locations
            .get_or_init(|| {
                crate::diagnostics::scene_membership_cells(self.cells.len());
                self.cells.iter().map(|cell| cell.location).collect()
            })
            .contains(&at)
    }
}

// Raw World-derived templates only. Observer knowledge, physical interaction,
// terminal policy and hallucination are deliberately absent from this payload.
#[derive(Debug)]
struct RawWorldFacts {
    world: tor_world::SharedIdentity<tor_world::World>,
    cells: tor_world::Shared<Vec<CellView>>,
    features: Vec<RawFeature>,
    exits: Vec<ExitView>,
    open_cells: BTreeSet<Location>,
}

#[derive(Debug)]
struct RawFeature {
    index: usize,
    door: Option<tor_world::Door>,
    secret: bool,
    door_base: bool,
    trap: Option<(u64, tor_world::TrapFamily)>,
}

const VIEW_EXIT_DIRECTIONS: [Direction; 6] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
    Direction::Up,
    Direction::Down,
];

impl RawWorldFacts {
    fn capture(game: &Game, snapshot: &CachedScene) -> Self {
        let indexes = snapshot.projection();
        crate::diagnostics::raw_world_facts(indexes.occurrences.len());
        let portable_lit = game.portable_lit_cells();
        let mut features = Vec::new();
        let cells = indexes
            .occurrences
            .iter()
            .enumerate()
            .map(|(index, &(location, frame))| {
                let door = game.world.door(location);
                let secret = game.world.secret_door(location).is_some();
                let trap = game
                    .world
                    .trap(location)
                    .map(|trap| (trap.id, trap.kind.family()));
                if door.is_some() || secret || trap.is_some() {
                    features.push(RawFeature {
                        index,
                        door,
                        secret,
                        door_base: door.is_some_and(|door| {
                            game.world.door_location(door.id) == Some(location)
                        }),
                        trap,
                    });
                }
                CellView {
                    object_presentation: None,
                    lit: game.world.is_lit(location) == Some(true)
                        || portable_lit.contains(&location),
                    frame,
                    door: None,
                    trap: None,
                    door_reachable: false,
                    door_secret: false,
                    door_approaches: Vec::new(),
                    material: match game.world.terrain(location) {
                        Some(tor_world::Terrain::Solid(material)) => material.name(),
                        _ if game.world.is_chamber(location.region) => "",
                        _ => "stone",
                    },
                    location,
                    wall: game.world.is_wall(location),
                    place_hint: game.world.has_place_hint(location),
                }
            })
            .collect();
        let open_cells: BTreeSet<_> = indexes
            .cells
            .iter()
            .copied()
            .filter(|location| !game.world.is_wall(*location))
            .collect();
        let exits = open_cells
            .iter()
            .flat_map(|&location| {
                VIEW_EXIT_DIRECTIONS
                    .into_iter()
                    .filter(move |direction| {
                        let stair = game.world.is_stair(location, *direction);
                        stair
                            || (!matches!(direction, Direction::Up | Direction::Down)
                                && game.world.passage(location, *direction).is_some())
                    })
                    .map(move |direction| ExitView {
                        location,
                        direction,
                    })
            })
            .collect();
        Self {
            world: game.world.weak_identity(),
            cells: tor_world::Shared::new(cells),
            features,
            exits,
            open_cells,
        }
    }

    fn project(
        &self,
        game: &Game,
        observer: ActorId,
        scene: &[tor_world::SightCell],
    ) -> (tor_world::Shared<Vec<CellView>>, Vec<ExitView>) {
        let actor = &game.actors[&observer];
        let mut cells = self.cells.clone();
        let contacts = std::cell::OnceCell::new();
        for feature in &self.features {
            let cell = &mut observation_cells_mut(&mut cells)[feature.index];
            let concealed = game.door_concealed(observer, feature.door, feature.secret);
            cell.door = (!concealed).then_some(feature.door).flatten();
            cell.door_secret = cell.door.is_some() && feature.secret;
            if concealed {
                cell.wall = true;
                cell.material = "stone";
            }
            if cell.door.is_some() && feature.door_base {
                cell.door_reachable = game.door_reachable_from(actor.location, cell.location);
                cell.door_approaches = game.disclosed_door_approaches(scene, cell.location);
            }
            cell.trap = feature
                .trap
                .filter(|(id, _)| {
                    game.feature_known(observer, tor_world::ConcealedFeatureId::Trap(*id))
                })
                .map(|(id, family)| TrapView {
                    id,
                    family: game
                        .feature_family_known(observer, tor_world::ConcealedFeatureId::Trap(id))
                        .then_some(family),
                    reachable: contacts
                        .get_or_init(|| game.trap_contacts_in_scene(observer, scene))
                        .contains(&cell.location),
                });
        }
        let mut exits = self.exits.clone();
        if let Some(policy) = game
            .unique_retrieval_policy()
            .filter(|policy| policy.protagonist == observer)
        {
            let terminal = ExitView {
                location: policy.exit.at,
                direction: policy.exit.direction,
            };
            if let Some(rank) = VIEW_EXIT_DIRECTIONS
                .iter()
                .position(|direction| *direction == terminal.direction)
                .filter(|_| self.open_cells.contains(&terminal.location))
            {
                let index = exits.partition_point(|exit| {
                    (
                        exit.location,
                        VIEW_EXIT_DIRECTIONS
                            .iter()
                            .position(|direction| *direction == exit.direction)
                            .expect("static exit direction"),
                    ) < (terminal.location, rank)
                });
                if exits.get(index) != Some(&terminal) {
                    exits.insert(index, terminal);
                }
            }
        }
        (cells, exits)
    }
}

type SceneEntries = std::collections::BTreeMap<ActorId, Arc<CachedScene>>;
#[derive(Clone, Debug, Default)]
pub(crate) struct ActorScenes(std::sync::Arc<std::sync::Mutex<SceneEntries>>);

impl PartialEq for ActorScenes {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for ActorScenes {}

/// Manhattan sight range in cells, measured from the eye cell.
pub(crate) const SIGHT_RANGE: u8 = 16;

impl crate::Actor {
    /// Personal artwork belongs to the birth body. A replacement body's visible
    /// artwork comes only from its current authored species, never a guessed ID.
    pub(crate) fn current_body_asset(&self) -> Option<String> {
        if let Some(creature) = self.combat.as_ref().map(|combat| combat.creature()) {
            if creature
                .build()
                .base_body_package()
                .zip(creature.build().active_body_package())
                .is_some_and(|(base, active)| base.id != active.id)
            {
                return creature
                    .build()
                    .content()
                    .detection_appearance(creature.build().species_id())
                    .and_then(|appearance| appearance.asset.as_deref())
                    .map(str::to_owned);
            }
        }
        self.asset.clone()
    }
    /// Effective Awareness governs nonvisual disclosure, not genuine sight.
    /// The source table is in feet; the user's explicit adaptation caps every
    /// tier, including 9+, at the unchanged ordinary sight limit.
    pub(crate) fn nonvisual_range(&self) -> u8 {
        let awareness = self.combat.as_ref().map_or(0, |combat| {
            combat
                .creature()
                .derived()
                .attributes
                .get(crate::attributes::Attribute::Awareness)
        });
        let feet = match awareness {
            0 => 5,
            1..=2 => 10,
            3..=4 => 20,
            5..=6 => 50,
            7..=8 => 100,
            _ => return SIGHT_RANGE,
        };
        (feet / tor_world::CELL_SIZE_FEET).min(u32::from(SIGHT_RANGE)) as u8
    }
}

/// Where the scene places the landing of the abstract stair the actor stands
/// on: straight up or down, just beyond any physically visible offset, so it
/// never collides with a real cell. Travel learns stair links from it.
pub(crate) fn stair_landing_offset(body: &crate::BodySpec, direction: Direction) -> Position {
    let [x, y, z] = body.eye;
    let body_z = body
        .cells
        .iter()
        .map(|cell| cell[2].abs())
        .max()
        .unwrap_or(0);
    // Nonvisual geometry may originate above/below the eye on a large body.
    // Keep the existing offset wherever it already exceeds that physical span.
    let beyond = i32::from(SIGHT_RANGE) + (x.abs() + y.abs() + z.abs()).max(body_z) + 1;
    Position {
        x: 0,
        y: 0,
        z: if direction == Direction::Down {
            -beyond
        } else {
            beyond
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupplyView {
    pub id: crate::content_ids::SupplyId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseAppearance {
    pub actor: ActorId,
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemView {
    pub corpse: Option<CorpseAppearance>,
    pub class: crate::ItemClass,
    pub description: String,
    pub id: ItemId,
    pub name: String,
    pub quantity: u64,
    pub appearance: String,
    pub identified: bool,
    pub known_supply: Option<SupplyView>,
    pub known_currency: Option<crate::CurrencyKind>,
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroundItemView {
    pub presentation: Option<crate::appearance_catalog::ObjectPresentation>,
    pub corpse: Option<CorpseAppearance>,
    pub class: crate::ItemClass,
    pub description: String,
    pub id: ItemId,
    pub name: String,
    pub quantity: u64,
    pub appearance: String,
    pub identified: bool,
    pub known_supply: Option<SupplyView>,
    pub known_currency: Option<crate::CurrencyKind>,
    /// Disclosed physical contact, independent of private holds.
    pub supply_reachable: bool,
    pub location: Location,
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownPlace {
    pub id: RegionId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorView {
    pub presentation: Option<crate::appearance_catalog::CreaturePresentation>,
    pub disclosure: crate::perception::ActorDisclosure,
    /// Empty when nothing names it.
    pub name: String,
    pub id: ActorId,
    pub location: Location,
    pub asset: Option<String>,
    /// Visibly worn and held items; empty unless its true form is disclosed.
    pub worn: Vec<WornView>,
}

/// One equipped item as any onlooker sees it worn or held: no more than it
/// would show lying on the ground. Weapons and armour are disclosed; rings,
/// amulets and other small or concealable things never are, so wearing them
/// changes nothing an observer receives.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WornView {
    pub slot: crate::content_ids::EquipmentSlot,
    /// Which of the wearer's sockets of that slot, counted in anatomy order.
    pub occurrence: u8,
    pub class: crate::ItemClass,
    pub appearance: String,
    pub asset: Option<String>,
}

/// Classes whose equipped items are visible on the body.
fn worn_visibly(class: crate::ItemClass) -> bool {
    matches!(class, crate::ItemClass::Weapon | crate::ItemClass::Armor)
}

impl Game {
    /// A wearer's visible equipment in canonical order, independent of
    /// inventory order, item identity and anything known only privately.
    pub(crate) fn worn_view(&self, wearer: &crate::Actor) -> Vec<WornView> {
        let slots = &wearer.anatomy.slots;
        let mut worn: Vec<WornView> = wearer
            .equipment
            .iter()
            .filter_map(|(socket, item)| {
                let index = usize::from(socket.0);
                let slot = slots.get(index)?;
                let item = &self.items.get(item)?.spec;
                worn_visibly(item.class).then(|| WornView {
                    slot: slot.clone(),
                    occurrence: slots[..index].iter().filter(|s| *s == slot).count() as u8,
                    class: item.class,
                    appearance: item.appearance.clone(),
                    asset: item.asset.clone(),
                })
            })
            .collect();
        worn.sort();
        worn
    }
}

/// Name projection after the caller establishes visible-actor eligibility.
/// Both ordinary perception and paid observations use this observer-aware seam.
/// Empty means unnamed, which clients present using their generic figure label.
pub(crate) fn projected_actor_name<'a>(
    _observer: &crate::Actor,
    subject: &'a crate::Actor,
) -> &'a str {
    let name = subject
        .combat
        .as_ref()
        .map_or("", |combat| combat.identity.name.as_str());
    if name.trim().is_empty() {
        ""
    } else {
        name
    }
}

/// An observable exit, without the unexplored destination's identity or contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitView {
    pub location: Location,
    pub direction: Direction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrapView {
    pub id: u64,
    pub family: Option<tor_world::TrapFamily>,
    pub reachable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellView {
    pub object_presentation: Option<crate::appearance_catalog::ObjectPresentation>,
    /// Illumination of this disclosed occurrence, including dark awareness.
    pub lit: bool,
    pub frame: u8,
    pub door: Option<tor_world::Door>,
    pub trap: Option<TrapView>,
    pub door_reachable: bool,
    pub door_secret: bool,
    pub door_approaches: Vec<Location>,
    pub material: &'static str,
    pub location: Location,
    pub wall: bool,
    pub place_hint: bool,
}

/// Mutate only an observation-owned projection, never a retained raw template.
/// Shared-handle clones copy no rows; the first actual COW detachment does.
pub(crate) fn observation_cells_mut(
    cells: &mut tor_world::Shared<Vec<CellView>>,
) -> &mut Vec<CellView> {
    if cells.is_shared() {
        crate::diagnostics::observation_cells_copied(cells.len());
    }
    cells
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotionView {
    pub velocity: [i64; 3],
    pub displaced: bool,
    pub impacted: bool,
}

/// Disclosed facts for one actor, separate from the authoritative game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub linked_holds: Vec<crate::linked_hold::LinkedHoldObservation>,
    pub interactions: Option<crate::interactions::InteractionView>,
    pub combat: Option<crate::combat::CombatView>,
    pub motion: Option<MotionView>,
    pub actor: ActorId,
    pub tick: u64,
    pub location: Location,
    pub region: Region,
    /// The observer's own asset, for views of itself from another angle.
    pub asset: Option<String>,
    /// What the observer visibly wears and holds, as onlookers see it.
    pub worn: Vec<WornView>,
    /// Immutable raw cells may share storage; edits detach this view's copy.
    pub visible_cells: tor_world::Shared<Vec<CellView>>,
    pub ground_items: Vec<GroundItemView>,
    pub apparent_objects: Vec<crate::perception::ApparentObjectView>,
    pub inventory: Vec<ItemView>,
    pub visible_actors: Vec<ActorView>,
    pub exits: Vec<ExitView>,
    pub known_places: Vec<KnownPlace>,
}

/// Ordinary map-learning appearance facts, without inventory, combat or charts.
/// Shares the canonical projection and cosmetic policy with full Observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapObservation {
    pub visible_cells: tor_world::Shared<Vec<CellView>>,
    pub ground_items: Vec<GroundItemView>,
    pub exits: Vec<ExitView>,
}

/// Minimal fresh ordinary actor-reference rows, without cosmetic or item facts.
/// Duplicate body rows and their canonical order match full Observation exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActorReferenceRow {
    pub id: ActorId,
    pub disclosure: crate::perception::ActorDisclosure,
}

pub(crate) struct ActorReferenceObservation {
    pub location: Location,
    pub visible_actors: Vec<ActorReferenceRow>,
}

/// One decision's disclosure plus the original illuminated eye-relative cells.
/// Tactile awareness and projected stair landings never become sight targeting.
pub(crate) struct DecisionSnapshot {
    pub view: Observation,
    /// Existing body-relative disclosure chart, including nonvisual awareness.
    pub scene: Arc<Vec<tor_world::SightCell>>,
    pub sight: Arc<Vec<tor_world::SightCell>>,
}

#[derive(Clone, Copy)]
enum ScenePurpose {
    Decision,
    Ordinary,
}

impl Game {
    pub(crate) fn decision_snapshot(&self, id: ActorId) -> Result<DecisionSnapshot, GameError> {
        let snapshot = self.scene_snapshot_shared_for(id, ScenePurpose::Decision)?;
        // Geometry and exactly witnessed classifications may be shared. Rich
        // entities, knowledge, effects and decision-visible facts remain fresh.
        let view = self.observe_in_snapshot(id, &snapshot)?;
        Ok(DecisionSnapshot {
            view,
            scene: snapshot.cells.clone(),
            sight: snapshot.sight.clone(),
        })
    }

    /// A free read using bounded exact three-dimensional sight through rotated
    /// portals, with ambient lighting and body-local awareness.
    /// Neither querying nor seeing through a portal counts as visiting a place.
    pub fn observe(&self, id: ActorId) -> Result<Observation, GameError> {
        self.observe_cached_scene(id).map(|(view, _)| view)
    }

    /// Construct fresh disclosure and return independently owned scene cells.
    /// Backend consumers that retain immutable geometry use `observe_shared_scene`.
    pub fn observe_scene(
        &self,
        id: ActorId,
    ) -> Result<(Observation, Vec<tor_world::SightCell>), GameError> {
        let (view, scene) = self.observe_shared_scene(id)?;
        crate::diagnostics::scene_cells_copied(scene.len());
        Ok((view, (*scene).clone()))
    }

    /// Fresh disclosure with shared geometry and exactly witnessed classifications.
    /// A caller that mutates its Arc-owned Vec must first detach its own copy.
    pub fn observe_shared_scene(
        &self,
        id: ActorId,
    ) -> Result<(Observation, Arc<Vec<tor_world::SightCell>>), GameError> {
        let (view, snapshot) = self.observe_cached_scene(id)?;
        Ok((view, snapshot.shared_cells()))
    }

    pub(crate) fn observe_cached_scene(
        &self,
        id: ActorId,
    ) -> Result<(Observation, Arc<CachedScene>), GameError> {
        let snapshot = self.scene_snapshot_shared(id)?;
        let view = self.observe_in_snapshot(id, &snapshot)?;
        Ok((view, snapshot))
    }

    /// Recompute current actor disclosure, borrowing only exact cached geometry.
    pub(crate) fn observe_actor_references(
        &self,
        id: ActorId,
    ) -> Result<ActorReferenceObservation, GameError> {
        let snapshot = self.scene_snapshot_shared(id)?;
        self.observe_actor_references_in_snapshot(id, &snapshot)
    }

    /// Same ordinary classifier/row ordering at a caller's exact immutable scene.
    pub(crate) fn observe_actor_references_in_snapshot(
        &self,
        id: ActorId,
        snapshot: &CachedScene,
    ) -> Result<ActorReferenceObservation, GameError> {
        let indexes = snapshot.projection();
        let perceived = snapshot.classified_actors(self, id)?;
        let location = self
            .actors
            .get(&id)
            .ok_or(GameError::UnknownActor)?
            .location;
        let visible_actors =
            self.project_actor_rows(id, &indexes.cells, &perceived, |id, disclosure, _| {
                move |_| ActorReferenceRow { id, disclosure }
            });
        crate::diagnostics::actor_reference_vector(visible_actors.len());
        Ok(ActorReferenceObservation {
            location,
            visible_actors,
        })
    }

    /// One canonical row filter/order shared by rich and reference-only views.
    /// The per-subject mapper is constructed once, preserving worn-view work.
    fn project_actor_rows<'a, T, F>(
        &'a self,
        observer: ActorId,
        cells: &BTreeSet<Location>,
        perceived: &crate::perception::PerceptionProjection,
        mut project: impl FnMut(ActorId, crate::perception::ActorDisclosure, &'a crate::Actor) -> F,
    ) -> Vec<T>
    where
        F: FnMut(Location) -> T,
    {
        let mut rows = Vec::new();
        for (&subject, perceived) in perceived {
            let actor = &self.actors[&subject];
            let Some(locations) =
                self.projected_actor_locations(observer, subject, cells, perceived)
            else {
                continue;
            };
            let mut project = project(subject, perceived.disclosure, actor);
            for at in locations {
                rows.push(project(at));
            }
        }
        rows
    }

    /// The exact ordinary row-existence rule, shared with event authority.
    /// A classification alone is insufficient: its body must produce a row,
    /// and an observer's own anchor is never such a row.
    fn projected_actor_locations<'a>(
        &'a self,
        observer: ActorId,
        subject: ActorId,
        cells: &'a BTreeSet<Location>,
        perceived: &'a crate::perception::PerceivedActor,
    ) -> Option<impl Iterator<Item = Location> + 'a> {
        if !self.actors[&subject].alive() || perceived.disclosure.reference().is_none() {
            return None;
        }
        let anchor = self.actors[&observer].location;
        Some(perceived.body.iter().filter_map(move |&(at, _)| {
            (cells.contains(&at) && (subject != observer || at != anchor)).then_some(at)
        }))
    }

    /// Query canonical ordinary reference authority without materializing rows.
    /// Callers supply one current classification and its exact scene cells.
    pub(crate) fn projected_actor_reference(
        &self,
        observer: ActorId,
        subject: ActorId,
        cells: &BTreeSet<Location>,
        projection: &crate::perception::PerceptionProjection,
    ) -> Option<crate::perception::ActorReferenceKind> {
        let perceived = projection.get(&subject)?;
        self.projected_actor_locations(observer, subject, cells, perceived)?
            .next()?;
        perceived.disclosure.reference()
    }

    pub(crate) fn attack_sound_alert(
        &self,
        listener: ActorId,
        source: Location,
        impact: Option<Location>,
    ) -> Result<Option<Location>, GameError> {
        let snapshot = self.scene_snapshot_shared(listener)?;
        let heard = snapshot.hearing_locations(self);
        Ok(if heard.contains(&source) {
            Some(source)
        } else {
            impact.filter(|at| heard.contains(at))
        })
    }

    /// Internal dependency membership includes apparent objects and concealed
    /// bodies, not merely actors for which a client reference can be minted.
    pub(crate) fn observation_depends_on_any(
        &self,
        observer: ActorId,
        subjects: &BTreeSet<ActorId>,
    ) -> Result<bool, GameError> {
        if subjects.is_empty() {
            return Ok(false);
        }
        let scene = self.scene_snapshot_shared(observer)?;
        Ok(scene
            .classified_actors(self, observer)?
            .keys()
            .any(|id| subjects.contains(id)))
    }

    /// Validate fallible observer projection inputs without constructing a view.
    /// This does not capture event authority or update observation knowledge.
    pub fn validate_observer_projection(&self, observer: ActorId) -> Result<(), GameError> {
        self.validate_observation_projection_inputs(observer)?;
        self.validate_event_observation_capacity(observer)
    }

    /// Fallible rich-projection checks not performed by minimal event facts.
    /// Keep this list adjacent to the canonical projection below.
    pub(crate) fn validate_observation_projection_inputs(
        &self,
        id: ActorId,
    ) -> Result<(), GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        self.world
            .region(actor.location.region)
            .ok_or(GameError::InvalidLocation)?;
        let validate_slot = |slot| {
            self.content
                .rules
                .equipment_slot(slot)
                .map(|_| ())
                .ok_or(GameError::Rule(
                    crate::abilities::RuleError::InvalidDefinition,
                ))
        };
        for slot in &actor.anatomy.slots {
            validate_slot(slot)?;
        }
        // An empty scene selects precisely the carried-item bucket.
        for (_, item) in self.items.perceived(id, &BTreeSet::new()) {
            if let Some(equipment) = &item.spec.equipment {
                validate_slot(&equipment.slot)?;
            }
        }
        self.projected_hallucination(id)?;
        Ok(())
    }

    fn project_item_identity<'a>(
        &self,
        id: ActorId,
        item: &'a crate::Item,
        eyes: &BTreeSet<Location>,
    ) -> (bool, bool, &'a str, Option<SupplyView>) {
        let actor = &self.actors[&id];
        let identified = if item.corpse.is_some() {
            actor.knowledge.contains(&item.spec.identity)
                || match item.location {
                    ItemLocation::Carried(owner) => owner == id,
                    ItemLocation::Ground(location) => eyes.contains(&location),
                }
        } else {
            !item.spec.concealed || actor.knowledge.contains(&item.spec.identity)
        };
        let anonymous_corpse = item.corpse.is_some() && !identified;
        let name = if identified {
            &item.spec.name
        } else if anonymous_corpse {
            "corpse"
        } else {
            &item.spec.appearance
        };

        let known_supply = identified
            .then(|| item.spec.supply.as_ref())
            .flatten()
            .and_then(|supply| self.content.rules.supply(supply))
            .map(|supply| SupplyView {
                id: supply.id.clone(),
                name: supply.name.clone(),
            });
        (identified, anonymous_corpse, name, known_supply)
    }

    fn project_ground_item(
        &self,
        id: ActorId,
        item_id: ItemId,
        item: &crate::Item,
        location: Location,
        eyes: &BTreeSet<Location>,
    ) -> GroundItemView {
        let (identified, anonymous_corpse, name, known_supply) =
            self.project_item_identity(id, item, eyes);
        GroundItemView {
            presentation: None,
            corpse: identified.then(|| item.corpse.clone()).flatten(),
            class: item.spec.class,
            description: item_description(name),
            id: item_id,
            name: name.to_owned(),
            quantity: item.quantity,
            appearance: if anonymous_corpse {
                "corpse".into()
            } else {
                item.spec.appearance.clone()
            },
            identified,
            supply_reachable: known_supply.is_some() && self.touches_supply(id, location),
            known_supply,
            known_currency: identified.then_some(item.spec.currency).flatten(),
            location,
            asset: (!anonymous_corpse)
                .then(|| item.spec.asset.clone())
                .flatten(),
        }
    }

    /// Synchronous canonical appearance projection for committed map learning.
    /// This does not acquire event receipts or prepare detection charts.
    pub fn observe_map_shared_scene(
        &self,
        id: ActorId,
    ) -> Result<(MapObservation, Arc<Vec<tor_world::SightCell>>), GameError> {
        self.validate_observation_projection_inputs(id)?;
        let snapshot = self.scene_snapshot_shared(id)?;
        let indexes = snapshot.projection();
        let mut ground_items = Vec::new();
        for (item_id, item) in self.items.perceived(id, &indexes.cells) {
            if let ItemLocation::Ground(location) = item.location {
                if indexes.cells.contains(&location) {
                    crate::diagnostics::item_view(item.spec.concealed);
                    ground_items.push(self.project_ground_item(
                        id,
                        item_id,
                        item,
                        location,
                        &indexes.eyes,
                    ));
                }
            }
        }
        let (visible_cells, exits) =
            snapshot
                .raw_world_facts(self)
                .project(self, id, &snapshot.cells);
        let mut view = MapObservation {
            visible_cells,
            ground_items,
            exits,
        };
        self.apply_hallucinated_map_presentation(id, &mut view, &snapshot.sight)?;
        self.validate_event_observation_capacity(id)?;
        Ok((view, snapshot.shared_cells()))
    }

    fn observe_in_snapshot(
        &self,
        id: ActorId,
        snapshot: &CachedScene,
    ) -> Result<Observation, GameError> {
        let _diagnostic_span = crate::diagnostics::spans::span("observe_projection");
        crate::diagnostics::observation();
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        let scene = snapshot.cells.as_slice();
        let sight = snapshot.sight.as_slice();
        let ProjectionIndexes { cells, eyes, .. } = snapshot.projection();
        let visible = |location: Location| cells.contains(&location);
        let perceived = snapshot.classified_actors(self, id)?;
        let mut ground_items = Vec::new();
        let mut inventory = Vec::new();
        let mut item_interactions = Vec::new();
        for (item_id, item) in self.items.perceived(id, cells) {
            crate::diagnostics::item_view(item.spec.concealed);
            if let ItemLocation::Ground(location) = item.location {
                if visible(location) {
                    ground_items.push(self.project_ground_item(id, item_id, item, location, eyes));
                }
                continue;
            }
            let (identified, anonymous_corpse, name, known_supply) =
                self.project_item_identity(id, item, eyes);
            match item.location {
                ItemLocation::Carried(owner) if owner == id => {
                    if item.spec.equipment.is_some()
                        || item.spec.projectile.is_some()
                        || item.spec.consumable.is_some()
                        || item.spec.edible.is_some()
                        || item.spec.readable.is_some()
                        || item.spec.device.is_some()
                    {
                        item_interactions.push(crate::interactions::ItemInteractionView {
                            portable_light: self.portable_light_view(id, item_id),
                            item: item_id,
                            slot: item
                                .spec
                                .equipment
                                .as_ref()
                                .map(|equipment| {
                                    self.content
                                        .rules
                                        .equipment_slot(&equipment.slot)
                                        .map(crate::EquipmentSlotView::from)
                                        .ok_or(GameError::Rule(
                                            crate::abilities::RuleError::InvalidDefinition,
                                        ))
                                })
                                .transpose()?,
                            equipped_slot: actor
                                .equipment
                                .iter()
                                .find_map(|(slot, item)| (*item == item_id).then_some(*slot)),
                            known_equipment: self.known_equipment_view(id, item_id),
                            known_projectile_bonus: self.known_projectile_bonus_view(id, item_id),
                            known_cursed: actor.known_equipment.curse(item_id),
                            known_protected: actor.known_equipment.protection(item_id),
                            drinkable: item.spec.consumable.is_some(),
                            readable: item.spec.readable.is_some(),
                            zappable: item.spec.device.is_some(),
                            device_activation: item.spec.device.as_ref().map(|device| {
                                if identified
                                    && self
                                        .content
                                        .rules
                                        .ability(&device.ability)
                                        .is_some_and(crate::abilities::self_device_definition_valid)
                                {
                                    crate::DeviceActivation::SelfTarget
                                } else if identified
                                    && self.content.rules.ability(&device.ability).is_some_and(
                                        crate::abilities::self_polymorph_device_definition_valid,
                                    )
                                {
                                    crate::DeviceActivation::DirectionOrSelf
                                } else {
                                    crate::DeviceActivation::Direction
                                }
                            }),
                            charges: actor.known_charges.observed(item_id, item.charges),
                            edible: item.spec.edible.is_some(),
                        });
                    }
                    inventory.push(ItemView {
                        corpse: identified.then(|| item.corpse.clone()).flatten(),
                        class: item.spec.class,
                        description: item_description(name),
                        id: item_id,
                        name: name.to_owned(),
                        quantity: item.quantity,
                        appearance: if anonymous_corpse {
                            "corpse".into()
                        } else {
                            item.spec.appearance.clone()
                        },
                        identified,
                        known_supply,
                        known_currency: identified.then_some(item.spec.currency).flatten(),
                        asset: (!anonymous_corpse)
                            .then(|| item.spec.asset.clone())
                            .flatten(),
                    });
                }
                _ => {}
            }
        }
        let completed: Vec<_> = self
            .event_log
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match event {
                crate::combat::CombatEvent::ItemCompleted { actor, work, .. }
                    if *actor == id && self.owns_event_receipt(id, index) =>
                {
                    Some(work.clone())
                }
                _ => None,
            })
            .collect();
        let refused: Vec<_> = self
            .event_log
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match event {
                crate::combat::CombatEvent::ItemRefused {
                    actor,
                    work,
                    reason,
                    ..
                } if *actor == id && self.owns_event_receipt(id, index) => {
                    Some(crate::ItemRefusalView {
                        work: work.clone(),
                        reason: *reason,
                    })
                }
                _ => None,
            })
            .collect();
        let curse_blocked: Vec<_> = self
            .event_log
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match event {
                crate::combat::CombatEvent::ItemCurseBlocked { actor, item, .. }
                    if *actor == id && self.owns_event_receipt(id, index) =>
                {
                    Some(*item)
                }
                _ => None,
            })
            .collect();
        let slots = actor
            .anatomy
            .slots
            .iter()
            .map(|id| {
                self.content
                    .rules
                    .equipment_slot(id)
                    .map(crate::EquipmentSlotView::from)
                    .ok_or(GameError::Rule(
                        crate::abilities::RuleError::InvalidDefinition,
                    ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let raw_world = snapshot.raw_world_facts(self);
        let (visible_cells, exits) = raw_world.project(self, id, scene);
        let mut view = Observation {
            linked_holds: self.linked_holds(id),
            interactions: (!actor.anatomy.slots.is_empty()
                || actor.pending.is_some()
                || actor.identification.is_some()
                || !item_interactions.is_empty()
                || !completed.is_empty()
                || !curse_blocked.is_empty()
                || !refused.is_empty())
            .then(|| crate::interactions::InteractionView {
                identification: self.identification_view(id),
                completed,
                curse_blocked,
                refused,
                slots,
                preparation: actor.pending.as_ref().map(|progress| {
                    crate::interactions::PreparationView {
                        work: progress.plan.work(),
                        remaining: if progress.active {
                            progress.remaining.saturating_sub(
                                self.actor_clock(id).saturating_sub(progress.started),
                            )
                        } else {
                            progress.remaining
                        },
                        active: progress.active,
                    }
                }),
                inventory: item_interactions,
            }),
            combat: self.combat_view(id, &perceived),
            motion: self.motion_view_active(id).then_some(MotionView {
                velocity: actor.motion.velocity,
                displaced: self.physics.displaced.contains(&id),
                impacted: self
                    .physics
                    .impacts
                    .iter()
                    .any(|e| e.entity == crate::PhysicsEntity::Actor(id)),
            }),
            actor: id,
            tick: self.tick,
            location: actor.location,
            asset: actor.current_body_asset(),
            worn: self.worn_view(actor),
            region: self
                .world
                .region(actor.location.region)
                .ok_or(GameError::InvalidLocation)?
                .clone(),
            ground_items,
            apparent_objects: perceived
                .iter()
                .filter_map(|(subject, perceived)| {
                    if perceived.disclosure != crate::perception::ActorDisclosure::ApparentObject
                        || !self.actors[subject].alive()
                    {
                        return None;
                    }
                    let state = &self.actors[subject];
                    visible(state.location).then(|| crate::perception::ApparentObjectView {
                        presentation: None,
                        subject: *subject,
                        location: state.location,
                        appearance: state.guise.appearance().expect("classified guise").clone(),
                    })
                })
                .collect(),
            visible_cells,
            inventory,
            visible_actors: self.project_actor_rows(
                id,
                cells,
                &perceived,
                |other_id, disclosure, other| {
                    let worn = if disclosure == crate::perception::ActorDisclosure::Creature {
                        self.worn_view(other)
                    } else {
                        Vec::new()
                    };
                    move |location| ActorView {
                        presentation: None,
                        disclosure,
                        name: if disclosure == crate::perception::ActorDisclosure::Creature {
                            projected_actor_name(actor, other).to_owned()
                        } else {
                            String::new()
                        },
                        id: other_id,
                        location,
                        asset: (disclosure == crate::perception::ActorDisclosure::Creature)
                            .then(|| other.current_body_asset())
                            .flatten(),
                        worn: worn.clone(),
                    }
                },
            ),
            exits,
            known_places: actor
                .visited
                .iter()
                .map(|&id| KnownPlace {
                    id,
                    name: self
                        .world
                        .known_region(id)
                        .expect("visited region is known")
                        .name
                        .clone(),
                })
                .collect(),
        };
        self.apply_hallucinated_presentation(&mut view, sight)?;
        Ok(view)
    }

    /// Closed undiscovered entrances disclose the same solid appearance as the wall.
    /// Opening remains one shared physical world change for every observer.
    // Retain the direct concealment witness used by the independent awareness tests.
    #[cfg(test)]
    fn concealed_door_wall(&self, actor: ActorId, location: Location) -> bool {
        self.door_concealed(
            actor,
            self.world.door(location),
            self.world.secret_door(location).is_some(),
        )
    }

    fn door_concealed(&self, actor: ActorId, door: Option<tor_world::Door>, secret: bool) -> bool {
        secret
            && door.is_some_and(|door| {
                !door.open
                    && !self.feature_known(actor, tor_world::ConcealedFeatureId::Door(door.id))
            })
    }

    fn trap_contacts_in_scene(
        &self,
        actor: ActorId,
        scene: &[tor_world::SightCell],
    ) -> BTreeSet<Location> {
        let mut contacts = self.melee_contact_locations_in_scene(actor, scene, None);
        if let Some(owner) = self.actors.get(&actor) {
            if let Some(body) = self.body_cells(owner.location, owner.orientation, &owner.body) {
                contacts.extend(body.into_iter().map(|(location, _)| location));
            }
        }
        contacts
    }

    pub(crate) fn trap_reachable(&self, actor: ActorId, location: Location) -> bool {
        self.scene_snapshot(actor).is_ok_and(|(scene, _)| {
            scene.iter().any(|cell| cell.location == location)
                && self
                    .trap_contacts_in_scene(actor, &scene)
                    .contains(&location)
        })
    }

    fn disclosed_door_approaches(
        &self,
        scene: &[tor_world::SightCell],
        door: Location,
    ) -> Vec<Location> {
        self.door_approaches_among(scene, door, true)
    }

    /// With `filtered`, only cells beside an occurrence of the door are
    /// considered. Tests compare that against considering every cell.
    fn door_approaches_among(
        &self,
        scene: &[tor_world::SightCell],
        door: Location,
        filtered: bool,
    ) -> Vec<Location> {
        let mut approaches = BTreeSet::new();
        // An approach needs the door seen one horizontal step away, so only
        // cells beside one of its occurrences can qualify. Checking that first
        // skips only candidates the final test below would reject.
        let doors: Vec<Position> = scene
            .iter()
            .filter(|cell| cell.location == door)
            .map(|cell| cell.offset)
            .collect();
        for from in scene {
            let beside = |x: i32, y: i32| {
                let target = Position {
                    x: from.offset.x + x,
                    y: from.offset.y + y,
                    z: from.offset.z,
                };
                doors.contains(&target)
            };
            if filtered && !(-1..=1).any(|x| (-1..=1).any(|y| (x, y) != (0, 0) && beside(x, y)))
                || !self.world.walkable(from.location)
            {
                continue;
            }
            for direction in Direction::HORIZONTAL {
                let (dx, dy, _) = direction.delta();
                if filtered && !beside(dx, dy) {
                    continue;
                }
                let local = direction.rotated(from.rotation);
                let reach = if let Some((a, b)) = direction.components() {
                    self.world.diagonal_reach(from.location, local, |side| {
                        !self.occupied(side)
                            && [a, b].into_iter().any(|first| {
                                let Some((next, turns)) = self
                                    .world
                                    .movement_neighbor(from.location, first.rotated(from.rotation))
                                else {
                                    return false;
                                };
                                let (sx, sy, _) = first.delta();
                                next == side
                                    && scene.iter().any(|seen| {
                                        seen.location == side
                                            && seen.rotation
                                                == tor_world::compose_rotation(from.rotation, turns)
                                            && seen.offset
                                                == Position {
                                                    x: from.offset.x + sx,
                                                    y: from.offset.y + sy,
                                                    z: from.offset.z,
                                                }
                                    })
                            })
                    })
                } else {
                    self.reach(from.location, local)
                };
                let Some((to, turns)) = reach else {
                    continue;
                };
                if to != door {
                    continue;
                }
                let rotation = tor_world::compose_rotation(from.rotation, turns);
                if scene.iter().any(|target| {
                    target.location == door
                        && target.rotation == rotation
                        && target.offset
                            == Position {
                                x: from.offset.x + dx,
                                y: from.offset.y + dy,
                                z: from.offset.z,
                            }
                }) {
                    approaches.insert(from.location);
                }
            }
        }
        approaches.into_iter().collect()
    }

    /// Backend-resolved view occurrences. A location may be seen at several
    /// offsets. Sight starts at the centre of the body's eye cell; offsets are
    /// relative to the actor's reference cell in its body frame.
    pub fn scene(&self, id: ActorId) -> Result<Vec<tor_world::SightCell>, GameError> {
        let mut sight = Vec::new();
        let scene = self.compute_scene(id, &mut |cells: &[tor_world::SightCell]| {
            sight.extend_from_slice(cells)
        })?;
        self.cache_scene(id, &scene, &sight)?;
        Ok(scene)
    }

    fn cache_scene(
        &self,
        id: ActorId,
        scene: &[tor_world::SightCell],
        sight: &[tor_world::SightCell],
    ) -> Result<(), GameError> {
        let witness = self.scene_witness(id)?;
        crate::diagnostics::scene_cells_copied(scene.len() + sight.len());
        self.insert_scene(
            id,
            Arc::new(CachedScene::new(witness, scene.to_vec(), sight.to_vec())),
        );
        Ok(())
    }

    fn insert_scene(&self, id: ActorId, scene: Arc<CachedScene>) {
        let mut cache = self.lifecycle.scenes.0.lock().expect("actor scene cache");
        if cache.len() >= 512 && !cache.contains_key(&id) {
            cache.pop_first();
        }
        cache.insert(id, scene);
    }

    pub(crate) fn scene_witness(&self, id: ActorId) -> Result<SceneWitness, GameError> {
        Ok(SceneWitness {
            world: self.world.perception_snapshot(),
            emitters: self.emitter_signature(),
            actor: self.actor_scene_witness(id)?,
        })
    }
    pub(crate) fn actor_scene_witness(&self, id: ActorId) -> Result<ActorSceneWitness, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        Ok(ActorSceneWitness {
            location: actor.location,
            orientation: actor.orientation,
            body: actor.body.clone(),
            disclosed: self.alive(id) || self.combat.selected == Some(id),
            sight: actor.perception().sight,
            nonvisual_range: actor.nonvisual_range(),
        })
    }

    /// Geometry and exact eye sight share one cache witness and one computation.
    /// The mutex is released before any projection, index construction or nested read.
    pub(crate) fn scene_snapshot_shared(&self, id: ActorId) -> Result<Arc<CachedScene>, GameError> {
        self.scene_snapshot_shared_for(id, ScenePurpose::Ordinary)
    }

    fn scene_snapshot_shared_for(
        &self,
        id: ActorId,
        purpose: ScenePurpose,
    ) -> Result<Arc<CachedScene>, GameError> {
        let witness = self.scene_witness(id)?;
        let cached = {
            let cache = self.lifecycle.scenes.0.lock().expect("actor scene cache");
            cache
                .get(&id)
                .filter(|cached| cached.witness == witness)
                .cloned()
        };
        if let Some(cached) = cached {
            return Ok(cached);
        }
        let mut sight = Vec::new();
        let scene =
            self.compute_scene_for(id, purpose, &mut |cells: &[tor_world::SightCell]| {
                sight.extend_from_slice(cells)
            })?;
        // Move fresh arrays into shared ownership. No element clone is needed.
        let snapshot = Arc::new(CachedScene::new(witness, scene, sight));
        self.insert_scene(id, snapshot.clone());
        Ok(snapshot)
    }

    pub(crate) fn scene_snapshot(
        &self,
        id: ActorId,
    ) -> Result<(Vec<tor_world::SightCell>, Vec<tor_world::SightCell>), GameError> {
        let snapshot = self.scene_snapshot_shared(id)?;
        crate::diagnostics::scene_cells_copied(snapshot.cells.len() + snapshot.sight.len());
        Ok(((*snapshot.cells).clone(), (*snapshot.sight).clone()))
    }

    /// Streaming's compatibility result owns only disclosed cells; eye sight stays shared.
    pub(crate) fn streaming_scene(
        &self,
        id: ActorId,
    ) -> Result<Vec<tor_world::SightCell>, GameError> {
        let snapshot = self.scene_snapshot_shared(id)?;
        crate::diagnostics::scene_cells_copied(snapshot.cells.len());
        Ok((*snapshot.cells).clone())
    }

    /// Ordinary disclosed-location membership. Physical contact is a separate gate.
    pub(crate) fn scene_contains(&self, id: ActorId, at: Location) -> Result<bool, GameError> {
        crate::diagnostics::scene_membership_lookup();
        Ok(self.scene_snapshot_shared(id)?.contains(at))
    }

    fn compute_scene(
        &self,
        id: ActorId,
        sight_observer: &mut impl FnMut(&[tor_world::SightCell]),
    ) -> Result<Vec<tor_world::SightCell>, GameError> {
        self.compute_scene_for(id, ScenePurpose::Ordinary, sight_observer)
    }

    fn compute_scene_for(
        &self,
        id: ActorId,
        purpose: ScenePurpose,
        sight_observer: &mut impl FnMut(&[tor_world::SightCell]),
    ) -> Result<Vec<tor_world::SightCell>, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        self.compute_scene_at(
            id,
            actor.location,
            actor.orientation,
            purpose,
            sight_observer,
        )
    }

    pub(crate) fn scene_snapshot_at_placement(
        &self,
        id: ActorId,
        placement: crate::abilities::Placement,
    ) -> Result<(Vec<tor_world::SightCell>, Vec<tor_world::SightCell>), GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if actor.location == placement.location && actor.orientation == placement.orientation {
            return self.scene_snapshot(id);
        }
        let mut sight = Vec::new();
        let scene = self.compute_scene_at(
            id,
            placement.location,
            placement.orientation,
            ScenePurpose::Ordinary,
            &mut |cells: &[tor_world::SightCell]| sight.extend_from_slice(cells),
        )?;
        Ok((scene, sight))
    }
    fn compute_scene_at(
        &self,
        id: ActorId,
        location: Location,
        orientation: u8,
        purpose: ScenePurpose,
        sight_observer: &mut impl FnMut(&[tor_world::SightCell]),
    ) -> Result<Vec<tor_world::SightCell>, GameError> {
        if !self.alive(id) && self.combat.selected != Some(id) && self.actors.contains_key(&id) {
            return Ok(Vec::new());
        }
        crate::diagnostics::scene();
        match purpose {
            ScenePurpose::Decision => crate::diagnostics::decision_scene(),
            ScenePurpose::Ordinary => crate::diagnostics::ordinary_scene(),
        }
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        let body = &actor.body;
        let Some((eye, eye_frame)) = self.eye_at(location, orientation, body) else {
            return Ok(Vec::new());
        };
        // The body frame is carried across any portal inside the body, so
        // eye-frame offsets are body-frame offsets from the eye cell.
        let [ex, ey, ez] = body.eye;
        let mut scene = if actor.perception().sight {
            self.illuminated_scene(eye, eye_frame, SIGHT_RANGE)
        } else {
            Vec::new()
        };
        sight_observer(&scene);
        for cell in &mut scene {
            cell.offset.x += ex;
            cell.offset.y += ey;
            cell.offset.z += ez;
        }
        let nonvisual = self.nonvisual_awareness_at(
            location,
            orientation,
            body,
            actor.nonvisual_range(),
            actor.perception().sight,
        );
        let local = merge_perception(
            self.local_awareness_at(location, orientation, body),
            nonvisual,
        );
        let mut scene = merge_perception(local, scene);
        // Abstract stair links are traversal, not geometry: standing on one
        // discloses its landing as a separate occurrence beyond physical sight.
        for direction in [Direction::Up, Direction::Down] {
            if self.world.is_stair(location, direction) {
                let Some(to) = self.world.link_destination(location, direction) else {
                    continue;
                };
                if !self.world.contains(to) {
                    continue;
                }
                scene.push(tor_world::SightCell {
                    physical: false,
                    location: to,
                    rotation: tor_world::compose_rotation(
                        orientation,
                        self.world.crossing_rotation(location, direction),
                    ),
                    offset: stair_landing_offset(body, direction),
                    wall: self.world.is_wall(to),
                });
            }
        }
        scene.sort_by_key(|c| c.offset);
        Ok(scene)
    }

    pub(crate) fn local_awareness(&self, id: ActorId) -> Vec<tor_world::SightCell> {
        let Some(actor) = self.actors.get(&id) else {
            return Vec::new();
        };
        self.local_awareness_at(actor.location, actor.orientation, &actor.body)
    }

    pub(crate) fn local_awareness_at(
        &self,
        location: Location,
        orientation: u8,
        body: &crate::BodySpec,
    ) -> Vec<tor_world::SightCell> {
        let mut scene = Vec::new();
        if let Some(occupied) = self.body_cells(location, orientation, body) {
            let mut occupied: Vec<_> = body.cells.iter().copied().zip(occupied).collect();
            occupied.sort_by_key(|(offset, _)| *offset);
            // Authored cell order must not let a neighbour projection replace
            // another occupied cell at a join with different reverse geometry.
            for &(offset, (location, frame)) in &occupied {
                scene.push(tor_world::SightCell {
                    physical: true,
                    location,
                    rotation: frame,
                    offset: Position {
                        x: offset[0],
                        y: offset[1],
                        z: offset[2],
                    },
                    wall: self.world.is_wall(location),
                });
            }
            for (offset, (location, frame)) in occupied {
                for mut cell in self.world.neighborhood_scene(location, frame) {
                    cell.offset.x += offset[0];
                    cell.offset.y += offset[1];
                    cell.offset.z += offset[2];
                    scene.push(cell);
                }
            }
        }
        scene
    }

    /// Unlike the preserved immediate neighbourhood, extended nonvisual
    /// perception obeys ordinary physical occlusion and Manhattan distance.
    /// Body origins preserve range for tall/wide actors and portal-local frames.
    /// This layer never enters the separate eye-sight vector.
    fn nonvisual_awareness_at(
        &self,
        location: Location,
        orientation: u8,
        body: &crate::BodySpec,
        range: u8,
        sight: bool,
    ) -> Vec<tor_world::SightCell> {
        if range <= 1 {
            return Vec::new();
        }
        let Some(occupied) = self.body_cells(location, orientation, body) else {
            return Vec::new();
        };
        let mut origins: Vec<_> = body.cells.iter().copied().zip(occupied).collect();
        origins.sort_by_key(|(offset, _)| *offset);
        let mut result = Vec::new();
        let portable_lit = self.portable_lit_cells();
        for (offset, (at, frame)) in origins {
            for mut cell in self.world.eye_scene(at, frame, range) {
                // Usable illuminated sight retains its normal eye-origin gate.
                // Blindness uses this same range, not a second reduced layer.
                if sight
                    && (self.world.is_lit(cell.location) == Some(true)
                        || portable_lit.contains(&cell.location))
                {
                    continue;
                }
                cell.offset.x += offset[0];
                cell.offset.y += offset[1];
                cell.offset.z += offset[2];
                result.push(cell);
            }
        }
        result
    }

    /// The geometry-only pair used by streaming; live relationship pins are
    /// deliberately computed by the settlement pass, not retained here.
    pub(crate) fn streaming_actor_regions(
        &self,
        id: ActorId,
    ) -> (BTreeSet<RegionId>, Vec<RegionId>) {
        let compute = || (self.reach_regions(id), self.perception_scene_regions(id));
        let cached = self.scene_witness(id).ok().and_then(|witness| {
            self.lifecycle
                .scenes
                .0
                .lock()
                .expect("actor scene cache")
                .get(&id)
                .filter(|snapshot| snapshot.witness == witness)
                .cloned()
        });
        // Never manufacture a scene solely for streaming or retain live pins.
        // The existing scene witness proves body, senses, world and emitters;
        // it deliberately over-invalidates this narrower geometric component.
        match cached {
            Some(snapshot) => snapshot.streaming.get_or_init(compute).clone(),
            None => compute(),
        }
    }

    /// Load-only geometry dependencies for exactly the actor scene above.
    /// Disclosure remains the merged scene; loading a blocker is not seeing it.
    pub(crate) fn perception_scene_regions(&self, id: ActorId) -> Vec<RegionId> {
        let Some(actor) = self.actors.get(&id) else {
            return Vec::new();
        };
        let mut regions = BTreeSet::new();
        if let Some((eye, frame)) = self.eye(id) {
            regions.extend(
                self.world
                    .illuminated_eye_scene_regions(eye, frame, SIGHT_RANGE)
                    .unwrap_or_default(),
            );
        }
        if !self.emitter_signature().0.is_empty() {
            regions.extend(self.light_required_regions());
            if let Some((eye, frame)) = self.eye(id) {
                regions.extend(
                    self.world
                        .eye_scene_regions(eye, frame, SIGHT_RANGE)
                        .unwrap_or_default(),
                );
            }
        }
        let range = actor.nonvisual_range();
        if range > 1 {
            if let Some(body) = self.body_cells(actor.location, actor.orientation, &actor.body) {
                for (at, frame) in body {
                    regions.extend(
                        self.world
                            .eye_scene_regions(at, frame, range)
                            .unwrap_or_default(),
                    );
                }
            }
        }
        regions.into_iter().collect()
    }

    /// The loaded actor's eye cell and frame, if its body resolves.
    pub(crate) fn eye(&self, id: ActorId) -> Option<(Location, u8)> {
        let actor = self.actors.get(&id)?;
        self.eye_at(actor.location, actor.orientation, &actor.body)
    }

    fn eye_at(
        &self,
        location: Location,
        orientation: u8,
        body: &crate::BodySpec,
    ) -> Option<(Location, u8)> {
        let eye_index = body.cells.iter().position(|cell| *cell == body.eye)?;
        self.body_cells(location, orientation, body)
            .map(|cells| cells[eye_index])
    }
}

/// Local awareness takes precedence over eye projection in the body chart.
/// Stable ordering keeps occupied cells (first in local awareness) authoritative.
pub(crate) fn merge_perception(
    mut local: Vec<tor_world::SightCell>,
    distant: Vec<tor_world::SightCell>,
) -> Vec<tor_world::SightCell> {
    local.extend(distant);
    local.sort_by_key(|cell| cell.offset);
    local.dedup_by_key(|cell| cell.offset);
    local
}

/// Initial authored appearance catalog. These cosmetic stubs do not add item rules.
/// Empty when nothing is authored; clients decide how to say so.
fn item_description(name: &str) -> String {
    match name {
        "copper token" => "A small copper disc, stamped with a worn spiral.",
        "silver token" => "A small silver disc, stamped with a worn spiral.",
        "iron token" => "A small iron disc, stamped with a worn spiral.",
        "stone tablet" => "A weathered slab of stone. Shallow marks run across its surface.",
        _ => "",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Extent, Passage, World};

    fn at(region: u64, x: i32, y: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        }
    }

    fn membership_fixture() -> (Game, ActorId) {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        (game, actor)
    }

    #[test]
    fn streaming_geometry_reuses_scene_witness_without_rebuilding_body_queries() {
        let (game, actor) = membership_fixture();
        game.scene_snapshot_shared(actor).unwrap();
        let expected = (
            game.reach_regions(actor),
            game.perception_scene_regions(actor),
        );
        assert_eq!(game.streaming_actor_regions(actor), expected);
        let before = crate::diagnostics::work_counts().body_cells;
        for _ in 0..4 {
            assert_eq!(game.streaming_actor_regions(actor), expected);
        }
        assert_eq!(
            crate::diagnostics::work_counts().body_cells - before,
            0,
            "an identical scene witness must reuse its geometric streaming queries"
        );
    }

    #[test]
    fn unchanged_actor_classification_is_shared_by_full_and_reference_views() {
        let (game, actor) = membership_fixture();
        let expected = game.observe(actor).unwrap();
        let before = crate::diagnostics::work_counts().perception_projections;
        assert_eq!(game.observe(actor).unwrap(), expected);
        let references = game.observe_actor_references(actor).unwrap();
        assert_eq!(
            references.visible_actors,
            expected
                .visible_actors
                .iter()
                .map(|row| ActorReferenceRow {
                    id: row.id,
                    disclosure: row.disclosure,
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(
            crate::diagnostics::work_counts().perception_projections - before,
            0,
            "complete and reference views share immutable classification, not later state"
        );
    }

    #[test]
    fn streaming_geometry_checks_candidate_body_and_world_before_reuse() {
        let (game, actor) = membership_fixture();
        let retained = game.scene_snapshot_shared(actor).unwrap();
        let original = game.streaming_actor_regions(actor);
        let mut candidate = game.clone();
        candidate.set_wall(at(1, 1, 0), true).unwrap();
        candidate.actors.get_mut(&actor).unwrap().orientation = 1;
        let expected = (
            candidate.reach_regions(actor),
            candidate.perception_scene_regions(actor),
        );
        assert_eq!(candidate.streaming_actor_regions(actor), expected);
        assert!(Arc::ptr_eq(
            &retained,
            &game.scene_snapshot_shared(actor).unwrap()
        ));
        assert_eq!(game.streaming_actor_regions(actor), original);
        let changed = candidate.scene_snapshot_shared(actor).unwrap();
        assert!(!Arc::ptr_eq(&retained, &changed));
        assert_eq!(candidate.streaming_actor_regions(actor), expected);
        let before = crate::diagnostics::work_counts().body_cells;
        assert_eq!(candidate.streaming_actor_regions(actor), expected);
        assert_eq!(crate::diagnostics::work_counts().body_cells, before);
    }

    #[test]
    fn classification_reuse_keeps_health_time_items_and_rejected_branches_fresh() {
        use crate::perception::contract_fixtures::{fixture_with, OWNER, SUBJECT};
        let mut game = fixture_with(&[], &[], |_, owner, _| {
            owner.modifiers.push(crate::rules::Modifier {
                cancellable: false,
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Set {
                    stat: crate::rules::Stat::Health,
                    value: 100,
                },
            });
        });
        let original = game.observe(OWNER).unwrap();
        let scene = game.scene_snapshot_shared(OWNER).unwrap();
        let classifications = scene.classified_actors(&game, OWNER).unwrap();
        assert!(game.health(OWNER).unwrap().0 > 1, "nonlethal fixture");
        let visibility = game.actors.visibility_epoch();
        crate::test_creatures::injure(&mut game, OWNER, 1).unwrap();
        game.tick += 1;
        let item = game
            .place_item(game.actors[&OWNER].location, "fresh fact".into())
            .unwrap();
        assert_eq!(game.actors.visibility_epoch(), visibility);
        assert!(Arc::ptr_eq(
            &classifications,
            &scene.classified_actors(&game, OWNER).unwrap()
        ));
        let fresh = game.observe(OWNER).unwrap();
        assert_eq!(fresh.tick, original.tick + 1);
        assert_eq!(
            fresh.combat.as_ref().unwrap().hp,
            original.combat.as_ref().unwrap().hp - 1
        );
        assert!(fresh.ground_items.iter().any(|row| row.id == item));
        let oracle = |game: &Game| {
            let mut uncached = game.clone();
            uncached.lifecycle.scenes = ActorScenes::default();
            uncached.observe(OWNER).unwrap()
        };
        assert_eq!(fresh, oracle(&game));
        let mut rejected = game.clone();
        crate::test_creatures::injure(&mut rejected, SUBJECT, u32::MAX).unwrap();
        assert!(!rejected.alive(SUBJECT));
        let absent = rejected.observe(OWNER).unwrap();
        assert!(absent.visible_actors.iter().all(|row| row.id != SUBJECT));
        assert_eq!(absent, oracle(&rejected));
        assert_eq!(
            game.observe(OWNER).unwrap(),
            fresh,
            "rejected epoch cannot poison retained facts"
        );
        assert_eq!(
            original.tick + 1,
            fresh.tick,
            "retained values remain immutable"
        );
    }

    #[test]
    fn complete_observations_retain_personal_facts_without_expanding_inspection() {
        use crate::perception::contract_fixtures::{OWNER, SUBJECT};
        let mut game = crate::hallucination::tests::named_fixture(&[]);
        let original = game.clone();
        let expected_stats = game.creature(OWNER).unwrap().own_stats();
        let before = crate::diagnostics::work_counts();
        let (view, scene, _) = game.observe_event_shared_scene(OWNER).unwrap();
        let (repeated, same_scene, _) = game.observe_event_shared_scene(OWNER).unwrap();
        assert_eq!(view, repeated);
        assert_eq!(scene, same_scene);
        assert!(view.visible_actors.iter().any(|row| row.id == SUBJECT));
        let captured = crate::diagnostics::work_counts();
        assert_eq!(
            captured.own_stats_materializations,
            before.own_stats_materializations
        );
        assert_eq!(
            captured.own_stats_static_entries_copied,
            before.own_stats_static_entries_copied
        );
        let retained_stats = view.combat.as_ref().unwrap().own_stats.as_ref().unwrap();
        assert_eq!(retained_stats.materialize(), expected_stats);
        assert_eq!(
            crate::diagnostics::work_counts().own_stats_materializations,
            captured.own_stats_materializations + 1
        );
        assert_eq!(game, original);
        game.apply_condition(
            OWNER,
            &crate::hallucination_clock::integration_tests::condition(),
            OWNER,
            1000,
        )
        .unwrap();
        let changed = game.observe(OWNER).unwrap();
        assert_ne!(changed, view);
        assert_eq!(
            changed
                .visible_actors
                .iter()
                .find(|row| row.id == SUBJECT)
                .unwrap()
                .name,
            "cosmetic c"
        );
        assert_ne!(
            changed
                .combat
                .as_ref()
                .unwrap()
                .own_stats
                .as_ref()
                .unwrap()
                .materialize(),
            expected_stats
        );
        assert_eq!(retained_stats.materialize(), expected_stats);
        assert_eq!(original.observe(OWNER).unwrap(), view);
        assert_eq!(game.rules_rng, original.rules_rng);
    }

    fn assert_direct_references_match_rows(game: &Game, observer: ActorId) {
        let snapshot = game.scene_snapshot_shared(observer).unwrap();
        let rows = game
            .observe_actor_references_in_snapshot(observer, &snapshot)
            .unwrap();
        let projection = snapshot.classified_actors(game, observer).unwrap();
        for subject in game.loaded_actor_ids() {
            let expected = rows
                .visible_actors
                .iter()
                .find(|row| row.id == subject)
                .and_then(|row| row.disclosure.reference());
            assert_eq!(
                game.projected_actor_reference(
                    observer,
                    subject,
                    snapshot.projection_cells(),
                    &projection,
                ),
                expected,
                "direct authority must match canonical rows for {subject:?}"
            );
        }
    }

    #[test]
    fn actor_reference_rows_share_canonical_body_order_and_exclude_self_anchor() {
        use crate::perception::contract_fixtures::{fixture_with_height, OWNER, SUBJECT};
        use crate::perception::ActorDisclosure::Creature;
        let mut game = fixture_with_height(&[], &[], 2, |_, _, _| {});
        for id in [OWNER, SUBJECT] {
            game.set_body(
                id,
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [0, 0, 1]],
                    eye: [0, 0, 1],
                    mass: 80,
                },
            )
            .unwrap();
        }
        let full = game.observe(OWNER).unwrap();
        let before = crate::diagnostics::work_counts();
        let references = game.observe_actor_references(OWNER).unwrap();
        assert_eq!(references.location, game.actors[&OWNER].location);
        // One own upper-body row, then the subject's two body rows. The own
        // anchor is excluded even though its location is ordinarily disclosed.
        assert_eq!(
            references.visible_actors,
            vec![
                ActorReferenceRow {
                    id: OWNER,
                    disclosure: Creature
                },
                ActorReferenceRow {
                    id: SUBJECT,
                    disclosure: Creature
                },
                ActorReferenceRow {
                    id: SUBJECT,
                    disclosure: Creature
                },
            ]
        );
        assert_eq!(
            references.visible_actors,
            full.visible_actors
                .iter()
                .map(|row| ActorReferenceRow {
                    id: row.id,
                    disclosure: row.disclosure
                })
                .collect::<Vec<_>>()
        );
        let after = crate::diagnostics::work_counts();
        assert_eq!(after.observations, before.observations);
        assert_eq!(
            after.perception_projections - before.perception_projections,
            0
        );
        assert_direct_references_match_rows(&game, OWNER);
    }

    #[test]
    fn actor_reference_rows_preserve_anonymous_hidden_and_guised_authority() {
        use crate::perception::contract_fixtures::{at, fixture, guise, OWNER, SUBJECT};
        use crate::perception::{ActorDisclosure, PerceptionValue};
        let mut invisible = fixture(&[], &[PerceptionValue::Invisible { enabled: true }]);
        assert_direct_references_match_rows(&invisible, OWNER);
        let nearby = invisible.observe_actor_references(OWNER).unwrap();
        assert_eq!(
            nearby.visible_actors,
            vec![ActorReferenceRow {
                id: SUBJECT,
                disclosure: ActorDisclosure::AnonymousContact,
            }]
        );
        let full = invisible.observe(OWNER).unwrap();
        assert_eq!(full.visible_actors.len(), 1);
        assert_eq!(
            full.visible_actors[0].disclosure,
            ActorDisclosure::AnonymousContact
        );
        invisible.teleport(SUBJECT, at(1, 6, 2)).unwrap();
        assert_direct_references_match_rows(&invisible, OWNER);
        assert!(invisible
            .observe_actor_references(OWNER)
            .unwrap()
            .visible_actors
            .is_empty());
        assert!(invisible.observe(OWNER).unwrap().visible_actors.is_empty());
        let detecting = fixture(
            &[PerceptionValue::SeeInvisible { enabled: true }],
            &[PerceptionValue::Invisible { enabled: true }],
        );
        assert_eq!(
            detecting
                .observe_actor_references(OWNER)
                .unwrap()
                .visible_actors,
            vec![ActorReferenceRow {
                id: SUBJECT,
                disclosure: ActorDisclosure::Creature
            }]
        );
        assert_direct_references_match_rows(&detecting, OWNER);
        let guised = fixture(&[], &[guise()]);
        assert_direct_references_match_rows(&guised, OWNER);
        assert!(guised
            .observe_actor_references(OWNER)
            .unwrap()
            .visible_actors
            .is_empty());
        let full = guised.observe(OWNER).unwrap();
        assert!(full.visible_actors.is_empty());
        assert_eq!(full.apparent_objects.len(), 1);
        assert_eq!(full.apparent_objects[0].subject, SUBJECT);
    }

    #[test]
    fn shared_hearing_geometry_keeps_physical_stair_and_source_priority_boundaries() {
        use crate::perception::contract_fixtures::{at, fixture, OWNER, SUBJECT};
        let mut game = fixture(&[], &[]);
        let origin = game.actors[&OWNER].location;
        let source = game.actors[&SUBJECT].location;
        let landing = at(2, 2, 2);
        game.register_named_anchors(
            RegionId(2),
            [("hearing_landing".into(), landing.position)].into(),
        )
        .unwrap();
        game.connect_named_stair(
            origin,
            Direction::Up,
            tor_world::NamedAnchor {
                region: RegionId(2),
                name: "hearing_landing".into(),
            },
        )
        .unwrap();
        let snapshot = game.scene_snapshot_shared(OWNER).unwrap();
        assert!(snapshot
            .cells
            .iter()
            .any(|cell| cell.location == landing && !cell.physical));
        let expected: BTreeSet<_> = game
            .world
            .eye_scene(origin, 0, 6)
            .into_iter()
            .filter(|cell| cell.physical)
            .map(|cell| cell.location)
            .collect();
        assert!(!expected.contains(&landing));
        assert_eq!(snapshot.hearing_locations(&game), &expected);
        assert_eq!(
            game.attack_sound_alert(OWNER, source, Some(origin))
                .unwrap(),
            Some(source)
        );
        assert_eq!(
            game.attack_sound_alert(OWNER, landing, Some(origin))
                .unwrap(),
            Some(origin)
        );
        assert_eq!(game.attack_sound_alert(OWNER, landing, None).unwrap(), None);
    }

    #[test]
    fn shared_hearing_geometry_is_immutable_across_rejected_candidate_locations() {
        use crate::perception::contract_fixtures::{at, fixture, OWNER};
        let game = fixture(&[], &[]);
        let original = game.clone();
        let retained = game.scene_snapshot_shared(OWNER).unwrap();
        let expected = retained.hearing_locations(&game).clone();
        let mut rejected = game.clone();
        rejected.teleport(OWNER, at(2, 2, 2)).unwrap();
        let changed = rejected.scene_snapshot_shared(OWNER).unwrap();
        let changed_locations = changed.hearing_locations(&rejected);
        let exact: BTreeSet<_> = rejected
            .world
            .eye_scene(at(2, 2, 2), 0, 6)
            .into_iter()
            .filter(|cell| cell.physical)
            .map(|cell| cell.location)
            .collect();
        assert_eq!(changed_locations, &exact);
        assert_ne!(changed_locations, &expected);
        assert_eq!(retained.hearing_locations(&game), &expected);
        let before = crate::diagnostics::work_counts();
        let restored = game.scene_snapshot_shared(OWNER).unwrap();
        assert_eq!(restored.hearing_locations(&game), &expected);
        assert_eq!(
            crate::diagnostics::work_counts().hearing_scene_builds - before.hearing_scene_builds,
            1
        );
        assert_eq!(game, original);
        assert_eq!(game.rules_rng, rejected.rules_rng);
    }

    #[test]
    fn shared_scene_geometry_indexes_initialize_once_per_payload() {
        let (game, actor) = membership_fixture();
        let snapshot = game.scene_snapshot_shared(actor).unwrap();
        assert!(!snapshot.cells.is_empty());
        assert!(!snapshot.sight.is_empty());
        assert!(snapshot.projection.get().is_none());
        let expected_occurrences: BTreeSet<_> = snapshot
            .cells
            .iter()
            .map(|cell| (cell.location, cell.rotation))
            .collect();
        let before = crate::diagnostics::work_counts();
        let indexes = snapshot.projection();
        assert_eq!(indexes.occurrences, expected_occurrences);
        let initialized = crate::diagnostics::work_counts();
        assert_eq!(
            initialized.projection_location_indexes - before.projection_location_indexes,
            2
        );
        assert_eq!(
            initialized.projection_location_cells - before.projection_location_cells,
            snapshot.cells.len() + snapshot.sight.len()
        );
        let retained = game.scene_snapshot_shared(actor).unwrap();
        assert!(Arc::ptr_eq(&snapshot, &retained));
        assert!(std::ptr::eq(indexes, retained.projection()));
        assert!(Arc::ptr_eq(&snapshot.cells, &retained.cells));
        assert!(Arc::ptr_eq(&snapshot.sight, &retained.sight));
        let reused = crate::diagnostics::work_counts();
        assert_eq!(
            reused.projection_location_indexes,
            initialized.projection_location_indexes
        );
        assert_eq!(reused.scene_cells_copied, before.scene_cells_copied);
    }

    #[test]
    fn shared_scene_geometry_never_caches_actor_or_item_disclosure() {
        let (mut game, actor) = membership_fixture();
        let (before, shared) = game.observe_shared_scene(actor).unwrap();
        let original = game.clone();
        let other = game
            .spawn_actor(at(1, 1, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        let item = game
            .place_item(at(1, 1, 0), "new shared-scene item".into())
            .unwrap();
        let (after, unchanged_geometry) = game.observe_shared_scene(actor).unwrap();
        assert!(Arc::ptr_eq(&shared, &unchanged_geometry));
        assert!(before.visible_actors.iter().all(|row| row.id != other));
        assert!(before.ground_items.iter().all(|row| row.id != item));
        assert!(after.visible_actors.iter().any(|row| row.id == other));
        assert!(after.ground_items.iter().any(|row| row.id == item));
        assert_eq!(original.observe(actor).unwrap(), before);
        assert_ne!(after, before);
    }

    #[test]
    fn shared_scene_eviction_releases_indexes_but_retained_arrays_remain_valid() {
        let (game, actor) = membership_fixture();
        let original = game.clone();
        let snapshot = game.scene_snapshot_shared(actor).unwrap();
        snapshot.projection();
        assert!(snapshot.contains(at(1, 0, 0)));
        assert!(!snapshot.hearing_locations(&game).is_empty());
        let raw = snapshot.raw_world_facts(&game);
        let weak_raw = Arc::downgrade(&raw);
        drop(raw);
        let retained_cells = snapshot.shared_cells();
        let expected_cells = (*retained_cells).clone();
        let weak = Arc::downgrade(&snapshot);
        // This exercises only the derived cache, not authoritative actor creation.
        for id in 2..=513 {
            game.insert_scene(
                ActorId(id),
                Arc::new(CachedScene::new(snapshot.witness.clone(), vec![], vec![])),
            );
        }
        {
            let cache = game.lifecycle.scenes.0.lock().unwrap();
            assert_eq!(cache.len(), 512);
            assert!(!cache.contains_key(&actor));
            assert!(cache.contains_key(&ActorId(2)));
        }
        drop(snapshot);
        assert!(
            weak_raw.upgrade().is_none(),
            "eviction also releases raw World facts"
        );
        assert!(
            weak.upgrade().is_none(),
            "array consumers must not pin cached indexes"
        );
        assert_eq!(*retained_cells, expected_cells);
        let before = crate::diagnostics::work_counts();
        let rebuilt = game.scene_snapshot_shared(actor).unwrap();
        assert_eq!(*rebuilt.cells, expected_cells);
        assert_eq!(crate::diagnostics::work_counts().scenes - before.scenes, 1);
        let cache = game.lifecycle.scenes.0.lock().unwrap();
        assert_eq!(cache.len(), 512);
        assert!(cache.contains_key(&actor));
        assert!(!cache.contains_key(&ActorId(2)));
        assert_eq!(game, original);
    }

    #[test]
    fn shared_scene_handles_are_immutable_across_rejected_candidate_witnesses() {
        let (game, actor) = membership_fixture();
        let original = game.clone();
        let (before, retained) = game.observe_shared_scene(actor).unwrap();
        let expected = (*retained).clone();
        let mut rejected = game.clone();
        rejected.set_cell_light(at(1, 2, 0), false).unwrap();
        let (changed, candidate_cells) = rejected.observe_shared_scene(actor).unwrap();
        assert_ne!(*candidate_cells, expected);
        assert_ne!(changed, before);
        assert_eq!(*retained, expected);
        let (restored_view, restored_cells) = game.observe_shared_scene(actor).unwrap();
        assert_eq!(restored_view, before);
        assert_eq!(*restored_cells, expected);
        assert!(!Arc::ptr_eq(&candidate_cells, &restored_cells));
        assert_eq!(game, original);
        assert_eq!(game.rules_rng, rejected.rules_rng);
    }

    #[test]
    fn owned_observation_scene_wrapper_copies_only_disclosed_cells() {
        let (game, actor) = membership_fixture();
        let (expected_view, shared) = game.observe_shared_scene(actor).unwrap();
        let before = crate::diagnostics::work_counts();
        let (view, mut owned) = game.observe_scene(actor).unwrap();
        assert_eq!(view, expected_view);
        assert_eq!(owned, *shared);
        assert_eq!(
            crate::diagnostics::work_counts().scene_cells_copied - before.scene_cells_copied,
            shared.len()
        );
        owned[0].offset.x += 100;
        let (_, repeated) = game.observe_shared_scene(actor).unwrap();
        assert!(Arc::ptr_eq(&shared, &repeated));
        assert_ne!(owned[0], repeated[0]);
    }

    #[test]
    fn raw_world_facts_track_hint_mutation_and_rejected_candidate_without_scene_change() {
        let (mut game, actor) = membership_fixture();
        let target = at(1, 1, 0);
        let baseline = game.observe(actor).unwrap();
        assert!(
            !baseline
                .visible_cells
                .iter()
                .find(|cell| cell.location == target)
                .unwrap()
                .place_hint
        );
        let scene = game.scene_snapshot_shared(actor).unwrap();
        let original = game.clone();
        let before = crate::diagnostics::work_counts().raw_world_fact_builds;
        game.world
            .set_named_place_hint(target, "fresh hint")
            .unwrap();
        assert!(Arc::ptr_eq(
            &scene,
            &game.scene_snapshot_shared(actor).unwrap()
        ));
        assert!(
            game.observe(actor)
                .unwrap()
                .visible_cells
                .iter()
                .find(|cell| cell.location == target)
                .unwrap()
                .place_hint
        );
        assert_eq!(
            crate::diagnostics::work_counts().raw_world_fact_builds - before,
            1
        );
        assert_eq!(
            original.observe(actor).unwrap(),
            baseline,
            "candidate-populated raw facts cannot cross the World witness"
        );
        drop(original);
        // A weak-only identity must also detach on mutation without a retained Game.
        game.world.set_place_hint(target, false).unwrap();
        assert_eq!(
            game.observe(actor).unwrap().visible_cells,
            baseline.visible_cells
        );
        assert!(Arc::ptr_eq(
            &scene,
            &game.scene_snapshot_shared(actor).unwrap()
        ));
    }

    fn raw_feature_fixture() -> (Game, ActorId) {
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_chamber(Region {
                id: RegionId(1),
                name: "raw facts".into(),
                bounds: Extent::new(5, 5, 1).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        (game, actor)
    }

    #[test]
    fn raw_world_features_keep_discovery_and_trap_family_fresh() {
        let (mut game, actor) = raw_feature_fixture();
        let door = at(1, 2, 1);
        let trap = at(1, 1, 1);
        // Warm absence before introducing physical features.
        assert!(game
            .observe(actor)
            .unwrap()
            .visible_cells
            .iter()
            .all(|cell| cell.door.is_none() && cell.trap.is_none()));
        game.world.place_door(door, 77, false, 1).unwrap();
        game.world
            .conceal_door(door, tor_world::Concealment { dc: 100 })
            .unwrap();
        game.world
            .place_trap(
                trap,
                tor_world::Trap {
                    id: 88,
                    dc: 100,
                    avoidance_dc: None,
                    kind: tor_world::TrapKind::Rust {
                        slot: "body_armor".into(),
                        amount: 1,
                    },
                },
            )
            .unwrap();
        let hidden = game.observe(actor).unwrap();
        let entrance = hidden
            .visible_cells
            .iter()
            .find(|cell| cell.location == door)
            .unwrap();
        assert_eq!(
            (entrance.door, entrance.wall, entrance.material),
            (None, true, "stone")
        );
        assert!(hidden
            .visible_cells
            .iter()
            .find(|cell| cell.location == trap)
            .unwrap()
            .trap
            .is_none());
        let world = game.world.weak_identity();
        let scene = game.scene_snapshot_shared(actor).unwrap();
        let retained_raw = scene.raw_world_facts(&game).cells.clone();
        let expected_raw = (*retained_raw).clone();
        let expected_hidden = (*hidden.visible_cells).clone();
        let before = crate::diagnostics::work_counts().raw_world_fact_builds;
        // Valid restored actor-owned facts, independent of projection helpers.
        game.actors.get_mut(&actor).unwrap().discovery = crate::discovery::read_discovery(serde_json::json!([
            {"feature":{"kind":"door","value":77},"attempted":false,"adjacent_attempted":false,"presence":true,"family":false},
            {"feature":{"kind":"trap","value":88},"attempted":false,"adjacent_attempted":false,"presence":true,"family":false}
        ])).unwrap();
        let before_projection = crate::diagnostics::work_counts().observation_cells_copied;
        let known = game.observe(actor).unwrap();
        let copied = crate::diagnostics::work_counts().observation_cells_copied - before_projection;
        let entrance = known
            .visible_cells
            .iter()
            .find(|cell| cell.location == door)
            .unwrap();
        assert_eq!(
            entrance.door,
            Some(tor_world::Door {
                id: 77,
                open: false,
                height: 1
            })
        );
        assert!(entrance.door_secret && entrance.door_reachable && !entrance.wall);
        assert_eq!(
            known
                .visible_cells
                .iter()
                .find(|cell| cell.location == trap)
                .unwrap()
                .trap,
            Some(TrapView {
                id: 88,
                family: None,
                reachable: true
            })
        );
        assert_eq!(
            copied,
            known.visible_cells.len(),
            "door and trap projection detach the raw cell array exactly once"
        );
        assert!(!known.visible_cells.shares_storage(&retained_raw));
        assert_eq!(*retained_raw, expected_raw);
        assert_eq!(*hidden.visible_cells, expected_hidden);
        game.reveal_feature_family(actor, tor_world::ConcealedFeatureId::Trap(88))
            .unwrap();
        assert_eq!(
            game.observe(actor)
                .unwrap()
                .visible_cells
                .iter()
                .find(|cell| cell.location == trap)
                .unwrap()
                .trap,
            Some(TrapView {
                id: 88,
                family: Some(tor_world::TrapFamily::Rust),
                reachable: true
            })
        );
        assert!(world.matches(&game.world));
        assert!(Arc::ptr_eq(
            &scene,
            &game.scene_snapshot_shared(actor).unwrap()
        ));
        assert_eq!(
            crate::diagnostics::work_counts().raw_world_fact_builds,
            before
        );
        game.world.set_door(door, true);
        assert!(
            game.observe(actor)
                .unwrap()
                .visible_cells
                .iter()
                .find(|cell| cell.location == door)
                .unwrap()
                .door
                .unwrap()
                .open
        );
    }

    #[test]
    fn raw_world_facts_never_cache_other_actor_door_clearance() {
        let (mut game, actor) = raw_feature_fixture();
        let door = at(1, 2, 2);
        let owner_at = game.actors[&actor].location;
        game.world.set_wall(at(1, 1, 2), true).unwrap();
        game.world.place_door(door, 77, true, 1).unwrap();
        let initial = game.observe(actor).unwrap();
        let entrance = initial
            .visible_cells
            .iter()
            .find(|cell| cell.location == door)
            .unwrap();
        assert!(entrance.door_reachable && entrance.door_approaches.contains(&owner_at));
        let scene = game.scene_snapshot_shared(actor).unwrap();
        let world = game.world.weak_identity();
        let blocker = game
            .spawn_actor(at(1, 2, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let blocked = game.observe(actor).unwrap();
        let entrance = blocked
            .visible_cells
            .iter()
            .find(|cell| cell.location == door)
            .unwrap();
        assert!(!entrance.door_reachable && !entrance.door_approaches.contains(&owner_at));
        assert!(world.matches(&game.world));
        assert!(Arc::ptr_eq(
            &scene,
            &game.scene_snapshot_shared(actor).unwrap()
        ));
        game.actors.get_mut(&blocker).unwrap().location = at(1, 4, 4);
        let reopened = game.observe(actor).unwrap();
        let entrance = reopened
            .visible_cells
            .iter()
            .find(|cell| cell.location == door)
            .unwrap();
        assert!(entrance.door_reachable && entrance.door_approaches.contains(&owner_at));
        assert_eq!(reopened.visible_cells, initial.visible_cells);
    }

    #[test]
    fn warm_observation_reuses_raw_world_facts_but_projects_fresh_entities() {
        let (mut game, actor) = membership_fixture();
        let cold = crate::diagnostics::work_counts();
        let expected = game.observe(actor).unwrap();
        assert!(
            !expected.visible_cells.is_empty(),
            "positive raw projection fixture"
        );
        let before = crate::diagnostics::work_counts();
        assert_eq!(before.raw_world_fact_builds - cold.raw_world_fact_builds, 1);
        assert_eq!(
            before.raw_world_fact_cells - cold.raw_world_fact_cells,
            expected.visible_cells.len()
        );
        assert_eq!(game.observe(actor).unwrap(), expected);
        let after = crate::diagnostics::work_counts();
        assert_eq!(after.observations - before.observations, 1);
        assert_eq!(
            after.perception_projections - before.perception_projections,
            0
        );
        assert_eq!(
            after.raw_world_fact_builds, before.raw_world_fact_builds,
            "an unchanged observation must reuse raw World facts, not rebuild them"
        );
        assert_eq!(after.raw_world_fact_cells, before.raw_world_fact_cells);
        let item = game
            .place_item(at(1, 1, 0), "fresh raw-cache item".into())
            .unwrap();
        let changed = game.observe(actor).unwrap();
        assert!(changed.ground_items.iter().any(|row| row.id == item));
        assert_eq!(changed.visible_cells, expected.visible_cells);
        assert_eq!(
            crate::diagnostics::work_counts().raw_world_fact_builds,
            before.raw_world_fact_builds
        );
    }

    #[test]
    fn warm_observation_shares_unmodified_cell_views_but_refreshes_entities() {
        let (mut game, actor) = raw_feature_fixture();
        let expected = game.observe(actor).unwrap();
        assert!(
            !expected.visible_cells.is_empty(),
            "positive feature-free cell projection fixture"
        );
        assert!(expected
            .visible_cells
            .iter()
            .all(|cell| cell.door.is_none() && cell.trap.is_none()));
        let before = crate::diagnostics::work_counts();
        assert_eq!(game.observe(actor).unwrap(), expected);
        let other = game
            .spawn_actor(at(1, 1, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        let item = game
            .place_item(at(1, 1, 0), "fresh shared-cell item".into())
            .unwrap();
        let fresh = game.observe(actor).unwrap();
        assert!(expected.visible_actors.iter().all(|row| row.id != other));
        assert!(expected.ground_items.iter().all(|row| row.id != item));
        assert!(fresh.visible_actors.iter().any(|row| row.id == other));
        assert!(fresh.ground_items.iter().any(|row| row.id == item));
        assert_eq!(fresh.visible_cells, expected.visible_cells);
        let after = crate::diagnostics::work_counts();
        assert_eq!(after.observations - before.observations, 2);
        assert_eq!(
            after.perception_projections - before.perception_projections,
            1
        );
        assert_eq!(after.scenes, before.scenes);
        assert_eq!(after.raw_world_fact_builds, before.raw_world_fact_builds);
        assert_eq!(
            after.observation_cells_copied - before.observation_cells_copied,
            0,
            "feature-free observations must not copy unchanged CellView rows"
        );
        assert!(expected.visible_cells.shares_storage(&fresh.visible_cells));

        // An independently retained caller view remains writable without
        // changing either the raw template or another observer snapshot.
        let mut edited = fresh.clone();
        let previous_hint = edited.visible_cells[0].place_hint;
        edited.visible_cells[0].place_hint = !previous_hint;
        assert!(!edited.visible_cells.shares_storage(&fresh.visible_cells));
        assert_eq!(fresh.visible_cells, expected.visible_cells);
        assert_ne!(edited.visible_cells, expected.visible_cells);
        assert_eq!(game.observe(actor).unwrap(), fresh);

        // Projection-owned mutations count the first detachment exactly once.
        let mut projected = fresh.visible_cells.clone();
        let before_detach = crate::diagnostics::work_counts().observation_cells_copied;
        observation_cells_mut(&mut projected)[0].place_hint = !previous_hint;
        let detached = crate::diagnostics::work_counts().observation_cells_copied;
        assert_eq!(detached - before_detach, fresh.visible_cells.len());
        observation_cells_mut(&mut projected)[0].place_hint = previous_hint;
        assert_eq!(
            crate::diagnostics::work_counts().observation_cells_copied,
            detached,
            "further mutations of the unique projection copy no rows"
        );
        assert_eq!(projected, expected.visible_cells);
        assert_eq!(fresh.visible_cells, expected.visible_cells);
    }

    #[test]
    fn decision_reuses_witnessed_geometry_but_refreshes_disclosure() {
        let (mut game, actor) = membership_fixture();
        let expected = game.observe(actor).unwrap();
        assert!(!expected.visible_cells.is_empty());
        let snapshot = game.scene_snapshot_shared(actor).unwrap();
        let before = crate::diagnostics::work_counts();
        let decision = game.decision_snapshot(actor).unwrap();
        assert_eq!(decision.view, expected);
        assert_eq!(*decision.sight, *snapshot.sight);
        let item = game
            .place_item(at(1, 1, 0), "fresh decision item".into())
            .unwrap();
        let fresh = game.decision_snapshot(actor).unwrap();
        assert!(fresh.view.ground_items.iter().any(|row| row.id == item));
        assert_eq!(fresh.view.visible_cells, expected.visible_cells);
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            after.perception_projections - before.perception_projections,
            0
        );
        assert_eq!(
            after.scenes, before.scenes,
            "warm AI decisions must not reconstruct identical witnessed geometry"
        );
        assert_eq!(after.raw_world_fact_builds, before.raw_world_fact_builds);
        assert!(Arc::ptr_eq(&snapshot.sight, &fresh.sight));
    }

    #[test]
    fn warm_observation_reuses_its_geometry_projection_indexes() {
        let (game, actor) = membership_fixture();
        let expected = game.observe(actor).unwrap();
        let before = crate::diagnostics::work_counts();
        assert_eq!(game.observe(actor).unwrap(), expected);
        let after = crate::diagnostics::work_counts();
        assert_eq!(after.observations - before.observations, 1);
        assert_eq!(
            after.perception_projections - before.perception_projections,
            0
        );
        assert_eq!(
            after.projection_location_indexes, before.projection_location_indexes,
            "unchanged scene and eye indexes are already owned by the cached projection"
        );
        assert_eq!(
            after.projection_location_cells,
            before.projection_location_cells
        );
        assert_eq!(
            after.scene_cells_copied, before.scene_cells_copied,
            "after complete accounting, fresh disclosure must borrow unchanged geometry"
        );
    }

    #[test]
    fn scene_cache_copy_counter_includes_both_arrays_at_install() {
        let (game, actor) = membership_fixture();
        let mut expected_sight = Vec::new();
        let expected_scene = game
            .compute_scene(actor, &mut |cells: &[tor_world::SightCell]| {
                expected_sight.extend_from_slice(cells)
            })
            .unwrap();
        assert!(!expected_scene.is_empty());
        assert!(!expected_sight.is_empty());
        game.lifecycle.scenes.0.lock().unwrap().clear();
        let before = crate::diagnostics::work_counts().scene_cells_copied;
        let (scene, sight) = game.scene_snapshot(actor).unwrap();
        assert_eq!(scene, expected_scene);
        assert_eq!(sight, expected_sight);
        assert_eq!(
            crate::diagnostics::work_counts().scene_cells_copied - before,
            expected_scene.len() + expected_sight.len(),
            "cache installation clones both independently owned arrays"
        );
    }

    #[test]
    fn scene_cache_copy_counter_includes_warm_snapshot_arrays() {
        let (game, actor) = membership_fixture();
        let (expected_scene, expected_sight) = game.scene_snapshot(actor).unwrap();
        assert!(!expected_scene.is_empty());
        assert!(!expected_sight.is_empty());
        let before = crate::diagnostics::work_counts();
        let (scene, sight) = game.scene_snapshot(actor).unwrap();
        assert_eq!(scene, expected_scene);
        assert_eq!(sight, expected_sight);
        let warm = crate::diagnostics::work_counts();
        assert_eq!(warm.scenes, before.scenes);
        assert_eq!(
            warm.scene_cells_copied - before.scene_cells_copied,
            expected_scene.len() + expected_sight.len(),
            "a warm full snapshot clones both cached arrays without building a scene"
        );
        assert_eq!(game.streaming_scene(actor).unwrap(), expected_scene);
        let streamed = crate::diagnostics::work_counts();
        assert_eq!(streamed.scenes, warm.scenes);
        assert_eq!(
            streamed.scene_cells_copied - warm.scene_cells_copied,
            expected_scene.len(),
            "streaming clones only disclosed cells, not the unused sight array"
        );
    }

    #[test]
    fn scene_build_counters_cover_cold_snapshots_placements_and_decisions() {
        let (game, actor) = membership_fixture();
        let before = crate::diagnostics::work_counts();
        game.scene_snapshot(actor).unwrap();
        let cold = crate::diagnostics::work_counts();
        assert_eq!(cold.scenes - before.scenes, 1);
        assert_eq!(cold.ordinary_scenes - before.ordinary_scenes, 1);
        assert_eq!(cold.decision_scenes, before.decision_scenes);
        game.scene_snapshot(actor).unwrap();
        let warm = crate::diagnostics::work_counts();
        assert_eq!(warm.scenes, cold.scenes);
        assert_eq!(warm.ordinary_scenes, cold.ordinary_scenes);
        game.scene_snapshot_at_placement(
            actor,
            crate::abilities::Placement {
                location: at(1, 1, 0),
                orientation: 0,
            },
        )
        .unwrap();
        let placed = crate::diagnostics::work_counts();
        assert_eq!(placed.scenes - warm.scenes, 1);
        assert_eq!(placed.ordinary_scenes - warm.ordinary_scenes, 1);
        game.lifecycle.scenes.0.lock().unwrap().clear();
        game.decision_snapshot(actor).unwrap();
        let decided = crate::diagnostics::work_counts();
        assert_eq!(decided.scenes - placed.scenes, 1);
        assert_eq!(decided.decision_scenes - placed.decision_scenes, 1);
        assert_eq!(decided.ordinary_scenes, placed.ordinary_scenes);
        game.decision_snapshot(actor).unwrap();
        let reused = crate::diagnostics::work_counts();
        assert_eq!(reused.scenes, decided.scenes);
        assert_eq!(reused.decision_scenes, decided.decision_scenes);
        assert_eq!(reused.ordinary_scenes, decided.ordinary_scenes);
        assert_eq!(
            decided.scenes - before.scenes,
            decided.ordinary_scenes - before.ordinary_scenes + decided.decision_scenes
                - before.decision_scenes
        );
    }

    #[test]
    fn phase5_scene_membership_warm_ground_fragments_do_not_copy_or_scan_the_scene() {
        let (mut game, actor) = membership_fixture();
        for id in 10..42 {
            game.place_item_stack(
                id,
                at(1, 1, 0),
                None,
                1,
                crate::ItemSpec::ordinary(format!("fragment {id}")),
            )
            .unwrap();
        }
        assert!(game.scene_contains(actor, at(1, 1, 0)).unwrap());
        let before = crate::diagnostics::work_counts();
        for id in 10..42 {
            assert!(game.supply_access(
                actor,
                &game.items[&ItemId(id)],
                &crate::rules::SupplyAccess::CarriedOrGroundTouch
            ));
        }
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            after.scene_membership_lookups - before.scene_membership_lookups,
            32
        );
        assert_eq!(after.scenes, before.scenes);
        assert_eq!(
            after.scene_cells_copied, before.scene_cells_copied,
            "warm fragment checks must not clone the disclosed scene"
        );
        assert_eq!(
            after.scene_membership_cells, before.scene_membership_cells,
            "warm fragment checks must not scan the disclosed scene"
        );
    }

    #[test]
    fn phase5_scene_membership_cold_index_is_built_once_and_is_read_only() {
        let (game, actor) = membership_fixture();
        let expected = game.compute_scene(actor, &mut |_| {}).unwrap();
        game.lifecycle.scenes.0.lock().unwrap().clear();
        let original = game.clone();
        let before = crate::diagnostics::work_counts();
        assert!(!game.scene_contains(actor, at(99, 0, 0)).unwrap());
        let cold = crate::diagnostics::work_counts();
        assert_eq!(cold.scenes - before.scenes, 1);
        assert_eq!(
            cold.scene_membership_cells - before.scene_membership_cells,
            expected.len()
        );
        for _ in 0..32 {
            assert!(!game.scene_contains(actor, at(99, 0, 0)).unwrap());
        }
        let warm = crate::diagnostics::work_counts();
        assert_eq!(warm.scenes, cold.scenes);
        assert_eq!(warm.scene_membership_cells, cold.scene_membership_cells);
        assert_eq!(warm.scene_cells_copied, cold.scene_cells_copied);
        assert_eq!(game, original);
    }

    fn check_membership(game: &Game, actor: ActorId) {
        let scene = game.compute_scene(actor, &mut |_| {}).unwrap();
        for location in
            scene
                .iter()
                .map(|cell| cell.location)
                .chain([at(1, 2, 0), at(2, 0, 1), at(99, 0, 0)])
        {
            assert_eq!(
                game.scene_contains(actor, location).unwrap(),
                scene.iter().any(|cell| cell.location == location)
            );
        }
    }

    #[test]
    fn phase5_scene_membership_tracks_witness_changes_and_candidate_rollback() {
        let (mut game, actor) = membership_fixture();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        check_membership(&game, actor);
        let mut candidate = game.clone();
        candidate.set_cell_light(at(1, 2, 0), false).unwrap();
        check_membership(&candidate, actor);
        check_membership(&game, actor);
        candidate.set_wall(at(1, 1, 0), true).unwrap();
        check_membership(&candidate, actor);
        candidate.set_wall(at(1, 1, 0), false).unwrap();
        candidate.teleport(actor, at(2, 0, 1)).unwrap();
        check_membership(&candidate, actor);
        candidate.actors.get_mut(&actor).unwrap().orientation = 1;
        check_membership(&candidate, actor);
        candidate.actors.get_mut(&actor).unwrap().orientation = 0;
        candidate.teleport(actor, at(1, 0, 0)).unwrap();
        candidate
            .set_body(
                actor,
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [1, 0, 0]],
                    eye: [1, 0, 0],
                    mass: 80,
                },
            )
            .unwrap();
        check_membership(&candidate, actor);
        candidate
            .apply_damage(
                actor,
                &crate::damage::DamageSpec::new(
                    vec![crate::damage::DamageComponent::fixed(
                        crate::combat::DamageType::Impact,
                        None,
                        1000,
                    )],
                    None,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(!candidate.alive(actor));
        check_membership(&candidate, actor);
        candidate.combat.selected = Some(actor);
        check_membership(&candidate, actor);
        candidate.combat.selected = None;
        check_membership(&candidate, actor);
        check_membership(&game, actor);
        // Eviction cannot turn the derived index into authority.
        game.lifecycle.scenes.0.lock().unwrap().clear();
        check_membership(&game, actor);
    }

    #[test]
    fn phase5_scene_membership_deduplicates_occurrences_without_granting_stair_touch() {
        let (mut game, actor) = membership_fixture();
        let landing = at(2, 0, 1);
        game.register_named_anchors(
            RegionId(2),
            std::collections::BTreeMap::from([("landing".into(), landing.position)]),
        )
        .unwrap();
        game.connect_named_stair(
            at(1, 0, 0),
            Direction::Up,
            tor_world::NamedAnchor {
                region: RegionId(2),
                name: "landing".into(),
            },
        )
        .unwrap();
        let scene = game.scene(actor).unwrap();
        assert!(scene.iter().any(|cell| cell.location == landing));
        assert!(game.scene_contains(actor, landing).unwrap());
        assert!(!game.touches_supply(actor, landing));
        let item = game
            .place_item_stack(
                10,
                landing,
                None,
                1,
                crate::ItemSpec::ordinary("stair supply".into()),
            )
            .unwrap();
        assert!(!game.supply_access(
            actor,
            &game.items[&item],
            &crate::rules::SupplyAccess::CarriedOrGroundTouch
        ));
        // Cache membership depends on physical locations, never occurrence offsets.
        // Explicitly exercise duplicate projected occurrences at distinct offsets.
        let mut repeated = scene.clone();
        let mut duplicate = repeated[0];
        duplicate.offset.x += 100;
        repeated.push(duplicate);
        game.cache_scene(actor, &repeated, &[]).unwrap();
        for cell in &repeated {
            assert!(game.scene_contains(actor, cell.location).unwrap());
        }
        assert_eq!(game.streaming_scene(actor).unwrap(), repeated);
    }

    #[test]
    fn phase5_scene_membership_index_respects_cache_capacity_and_eviction() {
        let (game, actor) = membership_fixture();
        assert!(game.scene_contains(actor, at(1, 0, 0)).unwrap());
        let cached = game.lifecycle.scenes.0.lock().unwrap()[&actor].clone();
        assert!(cached.locations.get().is_some());
        // Fill only this derived cache to its bound; no actors or claims are
        // synthesized in authoritative state.
        {
            let mut cache = game.lifecycle.scenes.0.lock().unwrap();
            cache.clear();
            for id in 2..=513 {
                cache.insert(ActorId(id), cached.clone());
            }
        }
        let original = game.clone();
        let before = crate::diagnostics::work_counts();
        assert!(game.scene_contains(actor, at(1, 0, 0)).unwrap());
        assert_eq!(crate::diagnostics::work_counts().scenes, before.scenes + 1);
        {
            let cache = game.lifecycle.scenes.0.lock().unwrap();
            assert_eq!(cache.len(), 512);
            assert!(cache.contains_key(&actor));
            assert!(!cache.contains_key(&ActorId(2)));
        }
        check_membership(&game, actor);
        assert_eq!(game, original);
    }

    #[test]
    fn observation_reuses_the_exact_scene_already_resolved_for_streaming() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        let scene = game.streaming_scene(actor).unwrap();
        let before = crate::diagnostics::work_counts();
        assert_eq!(game.observe_scene(actor).unwrap().1, scene);
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            after.scenes, before.scenes,
            "unchanged perception was projected twice"
        );
        assert_eq!(after.observations, before.observations + 1);
        let mut changed = game.clone();
        changed.set_cell_light(at(1, 2, 0), false).unwrap();
        let dark = changed.observe_scene(actor).unwrap().1;
        assert_ne!(dark, scene);
        assert_eq!(dark, changed.compute_scene(actor, &mut |_| {}).unwrap());
        assert_eq!(game.observe_scene(actor).unwrap().1, scene);
    }

    #[test]
    fn observation_scene_cache_tracks_actor_chart_body_and_death_disclosure() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        let check = |game: &Game| {
            let mut sight = Vec::new();
            let expected = game
                .compute_scene(actor, &mut |cells| sight.extend_from_slice(cells))
                .unwrap();
            let decision = game.decision_snapshot(actor).unwrap();
            assert_eq!(*decision.sight, sight);
            assert_eq!(decision.view, game.observe(actor).unwrap());
            assert_eq!(game.observe_scene(actor).unwrap().1, expected);
        };
        check(&game);
        game.teleport(actor, at(2, 0, 1)).unwrap();
        check(&game);
        game.actors.get_mut(&actor).unwrap().orientation = 1;
        check(&game);
        game.actors.get_mut(&actor).unwrap().orientation = 0;
        game.teleport(actor, at(1, 0, 0)).unwrap();
        game.set_body(
            actor,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0]],
                eye: [1, 0, 0],
                mass: 80,
            },
        )
        .unwrap();
        check(&game);
        game.apply_damage(
            actor,
            &crate::damage::DamageSpec::new(
                vec![crate::damage::DamageComponent::fixed(
                    crate::combat::DamageType::Impact,
                    None,
                    1000,
                )],
                None,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!game.alive(actor));
        assert!(game.observe_scene(actor).unwrap().1.is_empty());
        check(&game);
        game.combat.selected = Some(actor);
        assert!(!game.observe_scene(actor).unwrap().1.is_empty());
        check(&game);
        game.combat.selected = None;
        check(&game);
    }

    #[test]
    fn streaming_reuses_only_scenes_with_identical_perception_inputs() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        let initial = game.scene(actor).unwrap();
        let calls = crate::diagnostics::work_counts().scenes;
        assert_eq!(game.streaming_scene(actor).unwrap(), initial);
        assert_eq!(crate::diagnostics::work_counts().scenes, calls);
        let mut candidate = game.clone();
        let target = at(1, 2, 0);
        assert!(initial.iter().any(|cell| cell.location == target));
        candidate.set_cell_light(target, false).unwrap();
        let dark = candidate.streaming_scene(actor).unwrap();
        assert!(!dark.iter().any(|cell| cell.location == target));
        assert_eq!(crate::diagnostics::work_counts().scenes, calls + 1);
        assert_eq!(game.streaming_scene(actor).unwrap(), initial);
        candidate.set_cell_light(target, true).unwrap();
        candidate.set_wall(at(1, 1, 0), true).unwrap();
        assert!(!candidate
            .streaming_scene(actor)
            .unwrap()
            .iter()
            .any(|cell| cell.location == target));
        candidate.set_wall(at(1, 1, 0), false).unwrap();
        candidate
            .set_body(
                actor,
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [1, 0, 0]],
                    eye: [0, 0, 0],
                    mass: 80,
                },
            )
            .unwrap();
        candidate.set_cell_light(target, false).unwrap();
        // Extending the body puts the dark target in its immediate neighbourhood.
        assert!(candidate
            .streaming_scene(actor)
            .unwrap()
            .iter()
            .any(|cell| cell.location == target));
        assert_eq!(
            candidate.streaming_scene(actor).unwrap(),
            candidate.compute_scene(actor, &mut |_| {}).unwrap()
        );
    }

    #[test]
    fn door_approaches_ignore_only_cells_away_from_the_door() {
        let (mut compared, mut found) = (0, 0);
        for seed in 1..=24u64 {
            let mut state = 0x9E37_79B9_7F4A_7C15 ^ seed;
            let mut below = |n: u64| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state % n
            };
            let rooms = (1..=2)
                .map(|id| Region {
                    id: RegionId(id),
                    name: String::new(),
                    bounds: Extent::new(7, 7, 1).unwrap(),
                })
                .collect();
            let mut world = World::new(rooms, vec![]).unwrap();
            // A straight join and a rotated one, as in the latency fixture.
            for (from, direction, to, back, turns) in [
                (
                    at(1, 6, 3),
                    Direction::East,
                    at(2, 0, 3),
                    Direction::West,
                    0,
                ),
                (
                    at(1, 3, 0),
                    Direction::North,
                    at(2, 0, 5),
                    Direction::West,
                    1,
                ),
            ] {
                world
                    .connect(
                        Passage {
                            from,
                            direction,
                            to,
                        },
                        turns,
                    )
                    .unwrap();
                world
                    .connect(
                        Passage {
                            from: to,
                            direction: back,
                            to: from,
                        },
                        (4 - turns) % 4,
                    )
                    .unwrap();
            }
            let cell = |below: &mut dyn FnMut(u64) -> u64| {
                at(1 + below(2), below(7) as i32, below(7) as i32)
            };
            for _ in 0..12 {
                let _ = world.set_wall(cell(&mut below), true);
            }
            let mut game = Game::new(world, seed, crate::test_creatures::content());
            for _ in 0..8 {
                let _ = game.place_door(cell(&mut below), below(2) == 0, 1);
            }
            for _ in 0..4 {
                let _ = game.spawn_actor(cell(&mut below), NonZeroU64::new(100).unwrap());
            }
            for _ in 0..16 {
                let eye = cell(&mut below);
                let scene = game.world.eye_scene(eye, below(4) as u8, SIGHT_RANGE);
                let doors: BTreeSet<_> = scene
                    .iter()
                    .map(|c| c.location)
                    .filter(|&at| game.world.door(at).is_some())
                    .collect();
                for door in doors {
                    let approaches = game.door_approaches_among(&scene, door, true);
                    assert_eq!(
                        approaches,
                        game.door_approaches_among(&scene, door, false),
                        "seed {seed}: eye {eye:?}, door {door:?}"
                    );
                    compared += 1;
                    found += approaches.len();
                }
            }
        }
        assert!(
            compared > 200 && found > 200,
            "{compared} doors, {found} approaches"
        );
    }
}

#[cfg(test)]
#[path = "observation_awareness_tests.rs"]
mod awareness_tests;

#[cfg(test)]
#[path = "observation_body_tests.rs"]
mod body_tests;
