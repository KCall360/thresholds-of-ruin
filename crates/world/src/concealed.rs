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
                    "rust" => Ok(TrapFamily::Rust),
                    "sleeping_gas" => Ok(TrapFamily::SleepingGas),
                    _ => Err(E::unknown_variant(value, &["rust", "sleeping_gas"])),
                }
            }
        }
        d.deserialize_str(Visitor)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrapKind {
    Rust { slot: String, amount: u32 },
    SleepingGas { condition: String, duration: u64 },
}
impl TrapKind {
    pub fn family(&self) -> TrapFamily {
        match self {
            Self::Rust { .. } => TrapFamily::Rust,
            Self::SleepingGas { .. } => TrapFamily::SleepingGas,
        }
    }
    pub fn valid(&self) -> bool {
        let reference =
            |id: &str| !id.is_empty() && id.len() <= 80 && !id.chars().any(char::is_control);
        match self {
            Self::Rust { slot, amount } => reference(slot) && (1..=100).contains(amount),
            Self::SleepingGas {
                condition,
                duration,
            } => reference(condition) && (1..=1_000_000).contains(duration),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Trap {
    pub id: u64,
    pub dc: i32,
    pub kind: TrapKind,
}
impl<'de> Deserialize<'de> for Trap {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: u64,
            dc: i32,
            kind: TrapKind,
        }
        let Fields { id, dc, kind } = named_map(d)?;
        Ok(Self { id, dc, kind })
    }
}
impl Trap {
    pub fn valid(&self) -> bool {
        self.id > 0
            && self.id < u64::MAX
            && Concealment { dc: self.dc }.valid()
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
    pub fn place_trap(&mut self, at: Location, trap: Trap) -> Result<(), WorldError> {
        if !self.contains(at)
            || self.is_wall(at)
            || !self.trap_supported(at)
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
            "id":1,"dc":12,"kind":{"kind":"rust","slot":"body_armor","amount":1}
        });
        assert!(serde_json::from_value::<Trap>(trap.clone()).is_ok());
        assert!(serde_json::from_value::<Trap>(serde_json::json!([
            trap["id"],
            trap["dc"],
            trap["kind"]
        ]))
        .is_err());
        for field in ["id", "dc", "kind"] {
            let mut missing = trap.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<Trap>(missing).is_err());
        }
        let mut extra = trap;
        extra["extra"] = true.into();
        assert!(serde_json::from_value::<Trap>(extra).is_err());
    }
    #[test]
    fn trap_family_requires_its_canonical_string_representation() {
        for (family, name) in [
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
}
