//! Physical concealed-feature metadata. Knowledge and checks belong to simulation.
use super::*;
use serde::{Deserialize, Serialize};

fn named_map<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct Named<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Named<T> {
        type Value = T;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a named concealed-feature record")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Named(std::marker::PhantomData))
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ConcealedFeatureId {
    Door(u64),
    Trap(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Concealment {
    pub dc: i32,
}
impl<'de> Deserialize<'de> for Concealment {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            dc: i32,
        }
        let Fields { dc } = named_map(d)?;
        Ok(Self { dc })
    }
}
impl Concealment {
    pub fn valid(self) -> bool {
        (0..=100).contains(&self.dc)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrapFamily {
    TrapDoor,
    Arrow,
    BearTrap,
    Teleport,
    PoisonDart,
    Mysterious,
    Rust,
    SleepingGas,
}

impl<'de> Deserialize<'de> for TrapFamily {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = TrapFamily;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a canonical trap family string")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "trap_door" => Ok(TrapFamily::TrapDoor),
                    "arrow" => Ok(TrapFamily::Arrow),
                    "bear_trap" => Ok(TrapFamily::BearTrap),
                    "teleport" => Ok(TrapFamily::Teleport),
                    "poison_dart" => Ok(TrapFamily::PoisonDart),
                    "mysterious" => Ok(TrapFamily::Mysterious),
                    "rust" => Ok(TrapFamily::Rust),
                    "sleeping_gas" => Ok(TrapFamily::SleepingGas),
                    _ => Err(E::unknown_variant(
                        value,
                        &[
                            "trap_door",
                            "arrow",
                            "bear_trap",
                            "teleport",
                            "poison_dart",
                            "mysterious",
                            "rust",
                            "sleeping_gas",
                        ],
                    )),
                }
            }
        }
        d.deserialize_str(Visitor)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrapKind {
    TrapDoor {
        destination: NamedAnchor,
        open: bool,
    },
    Arrow {
        attack_bonus: i32,
        ammunition: String,
    },
    BearTrap {
        condition: String,
        duration: u64,
    },
    Teleport {},
    PoisonDart {
        attack_bonus: i32,
        exposure: String,
    },
    Mysterious {},
    Rust {
        slot: String,
        amount: u32,
    },
    SleepingGas {
        condition: String,
        duration: u64,
        #[serde(deserialize_with = "required_option")]
        exposure: Option<String>,
    },
}
impl TrapKind {
    pub fn family(&self) -> TrapFamily {
        match self {
            Self::TrapDoor { .. } => TrapFamily::TrapDoor,
            Self::Arrow { .. } => TrapFamily::Arrow,
            Self::BearTrap { .. } => TrapFamily::BearTrap,
            Self::Teleport { .. } => TrapFamily::Teleport,
            Self::PoisonDart { .. } => TrapFamily::PoisonDart,
            Self::Mysterious { .. } => TrapFamily::Mysterious,
            Self::Rust { .. } => TrapFamily::Rust,
            Self::SleepingGas { .. } => TrapFamily::SleepingGas,
        }
    }
    pub fn valid(&self) -> bool {
        let reference =
            |id: &str| !id.is_empty() && id.len() <= 80 && !id.chars().any(char::is_control);
        match self {
            Self::TrapDoor { destination, .. } => {
                destination.region.0 > 0
                    && destination.region.0 < u64::MAX
                    && valid_anchor_name(&destination.name)
            }
            Self::Arrow {
                attack_bonus,
                ammunition,
            } => (-100..=100).contains(attack_bonus) && reference(ammunition),
            Self::BearTrap {
                condition,
                duration,
            } => reference(condition) && (1..=1_000_000).contains(duration),
            Self::Teleport {} | Self::Mysterious {} => true,
            Self::PoisonDart {
                attack_bonus,
                exposure,
            } => (-100..=100).contains(attack_bonus) && reference(exposure),
            Self::Rust { slot, amount } => reference(slot) && (1..=100).contains(amount),
            Self::SleepingGas {
                condition,
                duration,
                exposure,
            } => {
                reference(condition)
                    && (1..=1_000_000).contains(duration)
                    && exposure.as_ref().is_none_or(|id| reference(id))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Trap {
    pub id: u64,
    pub dc: i32,
    pub avoidance_dc: Option<i32>,
    pub kind: TrapKind,
}
impl<'de> Deserialize<'de> for Trap {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: u64,
            dc: i32,
            #[serde(deserialize_with = "required_option")]
            avoidance_dc: Option<i32>,
            kind: TrapKind,
        }
        let Fields {
            id,
            dc,
            avoidance_dc,
            kind,
        } = named_map(d)?;
        Ok(Self {
            id,
            dc,
            avoidance_dc,
            kind,
        })
    }
}
impl Trap {
    pub fn valid(&self) -> bool {
        self.id > 0
            && self.id < u64::MAX
            && Concealment { dc: self.dc }.valid()
            && self
                .avoidance_dc
                .is_none_or(|dc| Concealment { dc }.valid())
            && self.kind.valid()
    }
}

impl World {
    pub fn has_concealed_features(&self) -> bool {
        !self.secret_doors.is_empty() || !self.traps.is_empty()
    }
    /// Authoritative immutable concealment at any cell occupied by a secret door.
    pub fn secret_door(&self, at: Location) -> Option<Concealment> {
        let (base, _) = self.door_entry(at)?;
        self.secret_doors.get(&base).copied()
    }
    /// Conceal an existing physical door, without changing its shared collision or sight.
    pub fn conceal_door(
        &mut self,
        at: Location,
        concealment: Concealment,
    ) -> Result<(), WorldError> {
        let (base, _) = self.door_entry(at).ok_or(WorldError::InvalidEndpoint)?;
        if !concealment.valid() || self.secret_doors.contains_key(&base) {
            return Err(WorldError::InvalidEndpoint);
        }
        self.secret_doors.insert(base, concealment);
        self.sight.region_changed(base.region);
        Ok(())
    }
    pub fn trap(&self, at: Location) -> Option<&Trap> {
        self.traps.get(&at)
    }
    pub fn trap_location(&self, id: u64) -> Option<Location> {
        self.traps
            .iter()
            .find_map(|(at, trap)| (trap.id == id).then_some(*at))
    }
    /// Pressure traps require a real downward solid face in their own region.
    /// Abstract stairs and support across a physical aperture never anchor a plate.
    pub fn trap_supported(&self, at: Location) -> bool {
        self.physics_neighbor(at, Direction::Down)
            .is_some_and(|(below, _)| below.region == at.region && self.is_wall(below))
    }
    /// Resolve structural coordinates without pretending absent terrain is loaded.
    pub fn named_anchor(&self, anchor: &NamedAnchor) -> Option<Location> {
        Some(Location {
            region: anchor.region,
            position: *self.named_anchors.get(&anchor.region)?.get(&anchor.name)?,
        })
    }
    /// Open lids retain their feature identity, but no longer require the support
    /// face they removed. The exact directed physical exit witnesses that state.
    pub fn trap_geometry_valid(&self, at: Location, trap: &Trap) -> bool {
        match &trap.kind {
            TrapKind::TrapDoor {
                destination,
                open: true,
            } => self.named_anchor(destination).is_some_and(|to| {
                self.physical_vertical.contains(&(at, Direction::Down))
                    && self
                        .passage(at, Direction::Down)
                        .is_some_and(|p| p.to == to)
                    && self.crossing_rotation(at, Direction::Down) == 0
                    && self.region(to.region).is_none_or(|_| {
                        self.physical_vertical.contains(&(to, Direction::Up))
                            && self.passage(to, Direction::Up).is_some_and(|p| p.to == at)
                            && self.crossing_rotation(to, Direction::Up) == 0
                    })
            }),
            TrapKind::TrapDoor {
                destination,
                open: false,
            } => {
                destination.region != at.region
                    && self.trap_supported(at)
                    && self.passage(at, Direction::Down).is_none()
            }
            _ => self.trap_supported(at),
        }
    }
    /// Permanently replace a supported lid with a reciprocal physical aperture.
    /// Both regions and the declared ceiling landing must already be resident.
    /// Whole-body collision/occupancy remains the physics owner's responsibility.
    pub fn open_trapdoor(&mut self, at: Location) -> Result<bool, WorldError> {
        let trap = self.trap(at).ok_or(WorldError::InvalidEndpoint)?;
        let TrapKind::TrapDoor { destination, open } = &trap.kind else {
            return Err(WorldError::InvalidEndpoint);
        };
        if *open {
            return Ok(false);
        }
        let to = self
            .named_anchor(destination)
            .ok_or(WorldError::InvalidEndpoint)?;
        if at.region == to.region || !self.trap_supported(at) || !self.walkable(to) {
            return Err(WorldError::InvalidEndpoint);
        }
        // A landing column must have real floor support and enough room for the
        // normal upright body; a lid may not open into a one-cell ceiling pocket.
        let height = self
            .region(to.region)
            .ok_or(WorldError::InvalidEndpoint)?
            .bounds
            .dimensions()
            .2;
        let mut cursor = to;
        let mut clearance = 1;
        let mut supported = false;
        for _ in 0..height {
            let Some((below, rotation)) = self.physics_neighbor(cursor, Direction::Down) else {
                break;
            };
            if below.region != to.region || rotation != 0 {
                break;
            }
            if self.is_wall(below) {
                supported = true;
                break;
            }
            if !self.walkable(below) {
                return Err(WorldError::InvalidEndpoint);
            }
            cursor = below;
            clearance += 1;
        }
        if !supported || clearance < 2 {
            return Err(WorldError::InvalidEndpoint);
        }
        let mut candidate = self.clone();
        candidate.connect_portal_area(
            Passage {
                from: at,
                direction: Direction::Down,
                to,
            },
            0,
            1,
            1,
        )?;
        candidate.connect_portal_area(
            Passage {
                from: to,
                direction: Direction::Up,
                to: at,
            },
            0,
            1,
            1,
        )?;
        let TrapKind::TrapDoor { open, .. } =
            &mut candidate.traps.get_mut(&at).expect("validated trap").kind
        else {
            unreachable!()
        };
        *open = true;
        *self = candidate;
        Ok(true)
    }
    pub fn place_trap(&mut self, at: Location, trap: Trap) -> Result<(), WorldError> {
        if !self.contains(at)
            || self.is_wall(at)
            || !self.trap_geometry_valid(at, &trap)
            || !trap.valid()
            || self.traps.contains_key(&at)
            || self.trap_location(trap.id).is_some()
        {
            return Err(WorldError::InvalidEndpoint);
        }
        self.traps.insert(at, trap);
        self.sight.region_changed(at.region);
        Ok(())
    }
    /// Region-bounded range iteration: unrelated floors are never scanned.
    pub fn traps_in_region(&self, region: RegionId) -> impl Iterator<Item = (Location, &Trap)> {
        let first = Location {
            region,
            position: Position {
                x: i32::MIN,
                y: i32::MIN,
                z: i32::MIN,
            },
        };
        self.traps
            .range(first..)
            .take_while(move |(at, _)| at.region == region)
            .map(|(at, trap)| (*at, trap))
    }
    pub fn secret_doors_in_region(
        &self,
        region: RegionId,
    ) -> impl Iterator<Item = (Location, Door, Concealment)> + '_ {
        let first = Location {
            region,
            position: Position {
                x: i32::MIN,
                y: i32::MIN,
                z: i32::MIN,
            },
        };
        self.secret_doors
            .range(first..)
            .take_while(move |(at, _)| at.region == region)
            .filter_map(|(at, concealment)| {
                self.doors.get(at).map(|door| (*at, *door, *concealment))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concealed_metadata_requires_named_complete_records() {
        assert!(serde_json::from_str::<Concealment>(r#"{"dc":12}"#).is_ok());
        for invalid in [r#"[12]"#, r#"{}"#, r#"{"dc":12,"extra":true}"#] {
            assert!(serde_json::from_str::<Concealment>(invalid).is_err());
        }
        let trap = serde_json::json!({
            "id":1,"dc":12,"avoidance_dc":null,"kind":{"kind":"rust","slot":"body_armor","amount":1}
        });
        assert!(serde_json::from_value::<Trap>(trap.clone()).is_ok());
        assert!(serde_json::from_value::<Trap>(serde_json::json!([
            trap["id"],
            trap["dc"],
            trap["kind"]
        ]))
        .is_err());
        for field in ["id", "dc", "avoidance_dc", "kind"] {
            let mut missing = trap.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<Trap>(missing).is_err());
        }
        let mut extra = trap;
        extra["extra"] = true.into();
        assert!(serde_json::from_value::<Trap>(extra).is_err());
    }
    #[test]
    fn exposure_reference_and_avoidance_metadata_are_strict_and_separate_from_discovery() {
        let kind = serde_json::json!({"kind":"sleeping_gas", "condition":"sleep", "duration":200, "exposure":null});
        assert!(serde_json::from_value::<TrapKind>(kind.clone())
            .unwrap()
            .valid());
        let mut missing = kind.clone();
        missing.as_object_mut().unwrap().remove("exposure");
        assert!(serde_json::from_value::<TrapKind>(missing).is_err());
        let mut invalid = kind;
        invalid["exposure"] = "".into();
        assert!(!serde_json::from_value::<TrapKind>(invalid).unwrap().valid());
        let trap = Trap {
            id: 1,
            dc: 100,
            avoidance_dc: Some(0),
            kind: TrapKind::Mysterious {},
        };
        assert!(trap.valid());
        assert!(!Trap {
            avoidance_dc: Some(101),
            ..trap
        }
        .valid());
        assert!(serde_json::from_value::<TrapKind>(
            serde_json::json!({"kind":"poison_dart", "attack_bonus":0, "poison_bonus":0})
        )
        .is_err());
    }
    #[test]
    fn trap_family_requires_its_canonical_string_representation() {
        for (family, name) in [
            (TrapFamily::TrapDoor, "trap_door"),
            (TrapFamily::Arrow, "arrow"),
            (TrapFamily::BearTrap, "bear_trap"),
            (TrapFamily::Teleport, "teleport"),
            (TrapFamily::PoisonDart, "poison_dart"),
            (TrapFamily::Mysterious, "mysterious"),
            (TrapFamily::Rust, "rust"),
            (TrapFamily::SleepingGas, "sleeping_gas"),
        ] {
            let value = serde_json::json!(name);
            assert_eq!(serde_json::to_value(family).unwrap(), value);
            assert_eq!(serde_json::from_value::<TrapFamily>(value).unwrap(), family);
            assert!(serde_json::from_value::<TrapFamily>(serde_json::json!({
                (name): null
            }))
            .is_err());
        }
    }
    fn fixture() -> (World, Location) {
        let at = Location {
            region: RegionId(1),
            position: Position { x: 2, y: 2, z: 0 },
        };
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_chamber(Region {
                id: at.region,
                name: "test".into(),
                bounds: Extent::new(5, 5, 2).unwrap(),
            })
            .unwrap();
        (world, at)
    }
    #[test]
    fn concealed_door_remains_opaque_and_colliding_until_opened() {
        let (mut world, at) = fixture();
        world.place_door(at, 1, false, 2).unwrap();
        world.conceal_door(at, Concealment { dc: 12 }).unwrap();
        let above = Location {
            position: Position {
                z: 1,
                ..at.position
            },
            ..at
        };
        assert_eq!(world.secret_door(above), Some(Concealment { dc: 12 }));
        assert!(world.opaque(at));
        assert!(!world.walkable(at));
        world.set_door(at, true);
        assert!(!world.opaque(at));
        assert!(world.walkable(at));
        assert_eq!(world.secret_door(at), Some(Concealment { dc: 12 }));
    }
    #[test]
    fn unsupported_space_does_not_become_a_pressure_floor() {
        let (mut world, at) = fixture();
        let unsupported = Location {
            position: Position {
                z: 1,
                ..at.position
            },
            ..at
        };
        let trap = Trap {
            id: 1,
            dc: 12,
            avoidance_dc: None,
            kind: TrapKind::Rust {
                slot: "body_armor".into(),
                amount: 1,
            },
        };
        let before = world.clone();
        assert!(world.place_trap(unsupported, trap).is_err());
        assert_eq!(world, before);
        assert!(!world.trap_supported(unsupported));
        assert!(world.trap_supported(at));
    }
    #[test]
    fn concealed_features_survive_detach_and_reject_duplicate_traps_atomically() {
        let (mut world, at) = fixture();
        world.place_door(at, 1, false, 2).unwrap();
        world.conceal_door(at, Concealment { dc: 12 }).unwrap();
        let trap = Trap {
            id: 1,
            dc: 11,
            avoidance_dc: None,
            kind: TrapKind::Rust {
                slot: "body_armor".into(),
                amount: 1,
            },
        };
        world.place_trap(at, trap.clone()).unwrap();
        let original = world.clone();
        assert!(world.place_trap(at, trap.clone()).is_err());
        assert_eq!(world, original);
        let slice = world.detach_region(at.region).unwrap();
        assert!(slice.valid());
        assert!(world.trap(at).is_none());
        world.attach_region(slice).unwrap();
        assert_eq!(world, original);
        assert_eq!(world.traps_in_region(at.region).count(), 1);
        assert_eq!(world.trap_location(1), Some(at));
    }
    fn trapdoor_fixture() -> (World, Location, Location) {
        let (mut world, from) = fixture();
        world
            .add_chamber(Region {
                id: RegionId(2),
                name: "lower".into(),
                bounds: Extent::new(5, 5, 2).unwrap(),
            })
            .unwrap();
        let to = Location {
            region: RegionId(2),
            position: Position { x: 3, y: 2, z: 1 },
        };
        world
            .register_named_anchors(
                to.region,
                std::collections::BTreeMap::from([("landing".into(), to.position)]),
            )
            .unwrap();
        world
            .place_trap(
                from,
                Trap {
                    id: 1,
                    dc: 12,
                    avoidance_dc: None,
                    kind: TrapKind::TrapDoor {
                        destination: NamedAnchor {
                            region: to.region,
                            name: "landing".into(),
                        },
                        open: false,
                    },
                },
            )
            .unwrap();
        (world, from, to)
    }
    #[test]
    fn named_anchor_requires_a_complete_named_map() {
        let anchor = NamedAnchor {
            region: RegionId(2),
            name: "landing".into(),
        };
        let named = serde_json::json!({"region":2, "name":"landing"});
        assert_eq!(serde_json::to_value(&anchor).unwrap(), named);
        assert_eq!(
            serde_json::from_value::<NamedAnchor>(named).unwrap(),
            anchor
        );
        for invalid in [
            r#"[2,"landing"]"#,
            r#"{"region":2}"#,
            r#"{"name":"landing"}"#,
            r#"{"region":2,"name":"landing","extra":true}"#,
            r#"{"region":2,"region":3,"name":"landing"}"#,
        ] {
            assert!(
                serde_json::from_str::<NamedAnchor>(invalid).is_err(),
                "noncanonical named anchor accepted: {invalid}"
            );
        }
    }
    #[test]
    fn trapdoor_destination_requires_named_map_in_runtime_world() {
        for open in [false, true] {
            let (mut world, from, _) = trapdoor_fixture();
            if open {
                world.open_trapdoor(from).unwrap();
            }
            assert!(world.checkpoint_valid(1));
            let encoded = serde_json::to_value(&world).unwrap();
            let decoded: World = serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(decoded, world);
            assert!(decoded.checkpoint_valid(1));
            let mut invalid = encoded;
            let destination = &mut invalid["traps"][0][1]["kind"]["destination"];
            assert_eq!(
                *destination,
                serde_json::json!({"region":2, "name":"landing"})
            );
            *destination = serde_json::json!([2, "landing"]);
            assert!(
                serde_json::from_value::<World>(invalid).is_err(),
                "runtime world accepted a positional trapdoor destination (open={open})"
            );
        }
    }
    #[test]
    fn trapdoor_destination_requires_named_map_in_runtime_region_slice() {
        for open in [false, true] {
            let (mut world, from, _) = trapdoor_fixture();
            if open {
                world.open_trapdoor(from).unwrap();
            }
            let slice = world.detach_region(from.region).unwrap();
            assert!(slice.valid());
            let encoded = serde_json::to_value(&slice).unwrap();
            let decoded: RegionSlice = serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(decoded, slice);
            assert!(decoded.valid());
            let mut invalid = encoded;
            let destination = &mut invalid["traps"][0][1]["kind"]["destination"];
            assert_eq!(
                *destination,
                serde_json::json!({"region":2, "name":"landing"})
            );
            *destination = serde_json::json!([2, "landing"]);
            assert!(
                serde_json::from_value::<RegionSlice>(invalid).is_err(),
                "runtime region slice accepted a positional trapdoor destination (open={open})"
            );
        }
    }
    #[test]
    fn opening_trapdoor_is_permanent_reciprocal_physical_topology_not_stairs() {
        let (mut world, from, to) = trapdoor_fixture();
        assert!(world.trap_supported(from));
        assert!(world.open_trapdoor(from).unwrap());
        assert!(!world.open_trapdoor(from).unwrap());
        assert!(!world.trap_supported(from));
        assert_eq!(world.physics_neighbor(from, Direction::Down), Some((to, 0)));
        assert_eq!(world.physics_neighbor(to, Direction::Up), Some((from, 0)));
        assert!(!world.is_stair(from, Direction::Down));
        assert!(!world.is_stair(to, Direction::Up));
        assert!(world.checkpoint_valid(1));
        assert!(world.historical_checkpoint_valid());
        let original = world.clone();
        let source = world.detach_region(from.region).unwrap();
        let target = world.detach_region(to.region).unwrap();
        assert!(source.valid() && target.valid());
        world.attach_region(target).unwrap();
        world.attach_region(source).unwrap();
        assert_eq!(world, original);
        assert!(world.checkpoint_valid(1));
        assert!(world.historical_checkpoint_valid());
    }
    #[test]
    fn trapdoor_does_not_mutate_until_exact_resident_clear_ceiling_is_ready() {
        let (mut world, from, to) = trapdoor_fixture();
        let target = world.detach_region(to.region).unwrap();
        let before = world.clone();
        assert!(world.open_trapdoor(from).is_err());
        assert_eq!(world, before);
        world.attach_region(target).unwrap();
        world
            .set_wall(
                Location {
                    position: Position {
                        z: 0,
                        ..to.position
                    },
                    ..to
                },
                true,
            )
            .unwrap();
        let before = world.clone();
        assert!(
            world.open_trapdoor(from).is_err(),
            "one-cell ceiling pocket is not a landing"
        );
        assert_eq!(world, before);
    }
}
