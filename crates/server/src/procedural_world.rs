//! Compact, pure structural descriptions. No terrain or mutable world state is
//! retained here; a query allocates at most one bounded generation group.
use crate::{
    generation_recipe::GenerationGroup,
    scenario_package::{Portal, RegionDef},
    Failure,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests {
    use super::*;
    fn family(domain: Domain) -> WorldFamily {
        WorldFamily {
            kind: FamilyKind::GridChain,
            version: 1,
            base_region: 100,
            domain,
            grid: [2, 3],
            size: [10, 8, 2],
            recipe: "rooms".into(),
            zone: None,
        }
    }
    #[test]
    fn open_descriptor_is_compact_and_random_access_is_order_independent() {
        let f = family(Domain::Open {
            max_depth: 1_000_000,
        });
        f.check().unwrap();
        let before = serde_json::to_vec(&f).unwrap();
        let a = f.describe("caves", 4, 5_999_999).unwrap().unwrap();
        let _ = f.describe("caves", 4, 100).unwrap().unwrap();
        assert_eq!(
            serde_json::to_vec(&a).unwrap(),
            serde_json::to_vec(&f.describe("caves", 4, 5_999_999).unwrap().unwrap()).unwrap()
        );
        assert_eq!(before, serde_json::to_vec(&f).unwrap());
        assert!(before.len() < 256);
    }
    #[test]
    fn seeded_domain_and_ports_vary_without_changing_handles() {
        let f = family(Domain::SeededFinite { min: 2, max: 20 });
        let lengths: std::collections::BTreeSet<_> =
            (0..32).map(|s| f.depth_limit("caves", s)).collect();
        let slots: std::collections::BTreeSet<_> = (0..32)
            .map(|s| f.port_region("caves", s, 1, "down"))
            .collect();
        assert!(lengths.len() > 1);
        assert!(slots.len() > 1);
        assert_eq!(f.address(106), Some((2, 0)));
    }
    #[test]
    fn shared_family_catalog_resolves_ports_owners_and_limits_consistently() {
        let families = BTreeMap::from([("caves".into(), family(Domain::Open { max_depth: 2 }))]);
        let catalog = FamilyCatalog::new(&families, 42);
        let up = catalog.resolve_endpoint("caves/2/up").unwrap();
        let region: u64 = up.split_once('/').unwrap().0.parse().unwrap();
        assert_eq!(catalog.owner(region), Some(106));
        assert_eq!(catalog.group_identity(106).as_deref(), Some("caves/2"));
        assert!(catalog.materializable(region));
        assert_eq!(catalog.owner(112), Some(112));
        assert!(!catalog.materializable(112));
        assert!(catalog.resolve_endpoint("caves/last/up").is_err());
        assert!(catalog.resolve_endpoint("unknown/1/up").is_err());
        assert!(catalog.resolve_endpoint("caves/1/unknown").is_err());
        assert_eq!(
            catalog.resolve_endpoint("9/authored").unwrap(),
            "9/authored"
        );
    }

    #[test]
    fn initial_entry_owner_and_cell_are_seeded_and_order_independent() {
        let f = family(Domain::Open { max_depth: 100 });
        let mut owners = std::collections::BTreeSet::new();
        let mut cells = std::collections::BTreeSet::new();
        for seed in 0..64 {
            let endpoint = f.resolve_port("caves", seed, "1", "start").unwrap();
            let owner = endpoint.split_once('/').unwrap().0.parse().unwrap();
            let first = f.describe("caves", seed, owner).unwrap().unwrap();
            let at = first.anchors["start"];
            assert!((1..f.size[0] - 1).contains(&at[0]));
            assert!((1..f.size[1] - 1).contains(&at[1]));
            assert_eq!(at[2], 0);
            let _ = f.describe("caves", seed, 100 + f.slots() * 50).unwrap();
            assert_eq!(
                f.describe("caves", seed, owner).unwrap().unwrap().anchors["start"],
                at
            );
            owners.insert(owner);
            cells.insert(at);
        }
        assert!(owners.len() > 1, "initial entry owner must vary");
        assert!(cells.len() > 1, "initial entry cell must vary");
    }

    #[test]
    fn invalid_bounds_and_overflow_are_rejected() {
        for domain in [
            Domain::Finite { count: 0 },
            Domain::SeededFinite { min: 4, max: 3 },
            Domain::Open { max_depth: 0 },
        ] {
            assert!(family(domain).check().is_err());
        }
        let mut f = family(Domain::Finite { count: 5 });
        f.base_region = u64::MAX;
        assert!(f.check().is_err());
        f.base_region = 100;
        f.grid = [0, 1];
        assert!(f.check().is_err());
    }
}
/// A chain of rectangular generation groups. Open families use a representable
/// u32 depth domain and an independent materialization budget, never a generated
/// final floor or objective.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldFamily {
    pub kind: FamilyKind,
    pub version: u32,
    pub base_region: u64,
    pub domain: Domain,
    pub grid: [u32; 2],
    pub size: [i32; 3],
    pub recipe: String,
    pub zone: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FamilyKind {
    GridChain,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Domain {
    Finite { count: u32 },
    SeededFinite { min: u32, max: u32 },
    Open { max_depth: u32 },
}
fn fail(message: &str) -> Failure {
    Failure::new(tor_protocol::ErrorCode::InvalidAction, message)
}
fn draw(identity: &str, seed: u64, depth: u32, stream: &str) -> u64 {
    let mut hash = Sha256::new();
    hash.update(b"tor-world-family-v1\0");
    hash.update((identity.len() as u64).to_le_bytes());
    hash.update(identity.as_bytes());
    hash.update(seed.to_le_bytes());
    hash.update(depth.to_le_bytes());
    hash.update(stream.as_bytes());
    u64::from_le_bytes(hash.finalize()[..8].try_into().expect("digest size"))
}
impl WorldFamily {
    fn address_depth_limit(&self) -> u32 {
        if matches!(self.domain, Domain::Open { .. }) {
            u32::MAX
        } else {
            self.max_depth()
        }
    }
    pub fn materializable(&self, identity: &str, seed: u64, region: u64) -> bool {
        self.address(region)
            .is_some_and(|(depth, _)| depth <= self.depth_limit(identity, seed))
    }
    /// Logical family/depth/port addresses resolve without generated coordinates.
    pub fn resolve_port(
        &self,
        identity: &str,
        seed: u64,
        depth: &str,
        port: &str,
    ) -> Result<String, Failure> {
        self.check()?;
        let depth = if depth == "last" && !matches!(self.domain, Domain::Open { .. }) {
            self.depth_limit(identity, seed)
        } else {
            depth
                .parse::<u32>()
                .map_err(|_| fail("Invalid procedural logical depth"))?
        };
        if depth == 0 || depth > self.depth_limit(identity, seed) {
            return Err(fail(
                "Procedural port lies outside domain or operational limit",
            ));
        }
        let region = match port {
            "up" | "down" => self.port_region(identity, seed, depth, port),
            "start" if depth == 1 => self.port_region(identity, seed, depth, "start-slot"),
            _ => return Err(fail("Unknown procedural port")),
        };
        Ok(format!("{region}/{port}"))
    }
    pub fn slots(&self) -> u64 {
        u64::from(self.grid[0]) * u64::from(self.grid[1])
    }
    pub fn max_depth(&self) -> u32 {
        match self.domain {
            Domain::Finite { count } => count,
            Domain::SeededFinite { max, .. } => max,
            Domain::Open { max_depth } => max_depth,
        }
    }
    pub fn check(&self) -> Result<(), Failure> {
        if self.version != 1
            || self.base_region == 0
            || self.grid.iter().any(|n| !(1..=8).contains(n))
            || self.size[0] < 5
            || self.size[0] > 32
            || self.size[1] < 5
            || self.size[1] > 32
            || self.size[2] != 2
            || self.recipe.is_empty()
            || self.max_depth() == 0
            || matches!(self.domain, Domain::Open { max_depth } if max_depth == u32::MAX)
            || matches!(self.domain, Domain::SeededFinite { min, max } if min == 0 || min > max)
            || self.last_region().is_none_or(|last| last == u64::MAX)
        {
            return Err(fail(
                "Invalid procedural family bounds, domain or identity range",
            ));
        }
        Ok(())
    }
    pub fn last_region(&self) -> Option<u64> {
        self.slots()
            .checked_mul(u64::from(self.address_depth_limit()))?
            .checked_sub(1)?
            .checked_add(self.base_region)
    }
    pub fn depth_limit(&self, identity: &str, seed: u64) -> u32 {
        match self.domain {
            Domain::SeededFinite { min, max } if min <= max => {
                min + (draw(identity, seed, 0, "domain") % (u64::from(max) - u64::from(min) + 1))
                    as u32
            }
            _ => self.max_depth(),
        }
    }
    /// Invert the stable arithmetic handle without materializing any members.
    pub fn address(&self, region: u64) -> Option<(u32, usize)> {
        if self.base_region == 0 {
            return None;
        }
        let offset = region.checked_sub(self.base_region)?;
        let slots = self.slots();
        if slots == 0 {
            return None;
        }
        let depth = offset / slots + 1;
        if depth > u64::from(self.address_depth_limit()) {
            return None;
        }
        Some((depth as u32, (offset % slots) as usize))
    }
    pub fn owner(&self, region: u64) -> Option<u64> {
        let (depth, _) = self.address(region)?;
        self.base_region
            .checked_add(u64::from(depth - 1).checked_mul(self.slots())?)
    }
    fn port_region(&self, identity: &str, seed: u64, depth: u32, port: &str) -> u64 {
        self.base_region
            + u64::from(depth - 1) * self.slots()
            + draw(identity, seed, depth, port) % self.slots()
    }
    pub fn group(&self, identity: &str, seed: u64, depth: u32) -> Result<GenerationGroup, Failure> {
        self.check()?;
        if depth == 0 || depth > self.depth_limit(identity, seed) {
            return Err(fail(
                "Procedural address exceeds its domain or operational depth limit",
            ));
        }
        let first = self.base_region + u64::from(depth - 1) * self.slots();
        Ok(GenerationGroup {
            depth,
            recipe: self.recipe.clone(),
            grid: self.grid,
            members: (first..first + self.slots()).collect(),
        })
    }
    /// Structural source only: empty walls, entities and generated coordinates.
    pub fn describe(
        &self,
        identity: &str,
        seed: u64,
        region: u64,
    ) -> Result<Option<RegionDef>, Failure> {
        self.check()?;
        let Some((depth, slot)) = self.address(region) else {
            return Ok(None);
        };
        if !matches!(self.domain, Domain::Open { .. }) && depth > self.depth_limit(identity, seed) {
            return Ok(None);
        }
        let [w, h, _] = self.size;
        let mut anchors = BTreeMap::from([
            ("west".into(), [0, 0, 0]),
            ("east".into(), [w - 1, 0, 0]),
            ("north".into(), [0, 0, 0]),
            ("south".into(), [0, h - 1, 0]),
        ]);
        if depth == 1 && self.port_region(identity, seed, depth, "start-slot") == region {
            // Entry is final structural data, not a provisional carved position.
            // The room stage contains this anchor and stair cells exclude it.
            let x = 1 + (draw(identity, seed, depth, "start-x") % (w - 2) as u64) as i32;
            let y = 1 + (draw(identity, seed, depth, "start-y") % (h - 2) as u64) as i32;
            anchors.insert("start".into(), [x, y, 0]);
        }
        let cols = self.grid[0] as usize;
        let rows = self.grid[1] as usize;
        let mut portals = Vec::new();
        for (exists, direction, opposite, target, at, width) in [
            (
                slot % cols > 0,
                "west",
                "east",
                region.saturating_sub(1),
                [0, 0, 0],
                h,
            ),
            (
                slot % cols + 1 < cols,
                "east",
                "west",
                region.saturating_add(1),
                [w - 1, 0, 0],
                h,
            ),
            (
                slot / cols > 0,
                "north",
                "south",
                region.saturating_sub(cols as u64),
                [0, 0, 0],
                w,
            ),
            (
                slot / cols + 1 < rows,
                "south",
                "north",
                region.saturating_add(cols as u64),
                [0, h - 1, 0],
                w,
            ),
        ] {
            if exists {
                portals.push(Portal {
                    rotation: None,
                    kind: Some("boundary".into()),
                    at,
                    direction: direction.into(),
                    to: format!("{target}/{opposite}"),
                    turns: 0,
                    width: width as u16,
                    height: 2,
                });
            }
        }
        let mut stair_anchors = Vec::new();
        if self.port_region(identity, seed, depth, "up") == region {
            stair_anchors.push("up".into());
        }
        if self.port_region(identity, seed, depth, "down") == region {
            stair_anchors.push("down".into());
        }
        Ok(Some(RegionDef {
            lit: true,
            lighting: vec![],
            id: region,
            name: format!("{identity} depth {depth}, slot {}", slot + 1),
            size: self.size,
            chamber: true,
            zone: self.zone.clone(),
            gravity: None,
            gravity_overrides: vec![],
            anchors,
            walls: vec![],
            openings: vec![],
            places: vec![],
            portals,
            doors: vec![],
            items: vec![],
            actors: vec![],
            generate: Some(crate::generator::Generate {
                group: Some(format!("{identity}/{depth}")),
                boundary_anchors: vec![
                    "west".into(),
                    "east".into(),
                    "north".into(),
                    "south".into(),
                ],
                generator: "group".into(),
                version: 1,
                salt: 0,
                rooms: [1, 1],
                stair_anchors,
                actors: None,
                items: None,
            }),
        }))
    }
    /// Reciprocal chain connections resolved without choosing terrain cells.
    pub fn connections(&self, identity: &str, seed: u64, region: u64) -> Vec<StairConnection> {
        if self.check().is_err() {
            return vec![];
        }
        let Some((depth, _)) = self.address(region) else {
            return vec![];
        };
        let mut out = Vec::new();
        let limit = if matches!(self.domain, Domain::Open { .. }) {
            self.address_depth_limit()
        } else {
            self.depth_limit(identity, seed)
        };
        if depth > limit {
            return out;
        }
        if depth > 1 && self.port_region(identity, seed, depth, "up") == region {
            let destination = self.port_region(identity, seed, depth - 1, "down");
            out.push(StairConnection {
                id: format!("{identity}/{}", depth - 1),
                from: format!("{region}/up"),
                direction: tor_world::Direction::Up,
                to: format!("{destination}/down"),
                destination,
            });
        }
        if depth < limit && self.port_region(identity, seed, depth, "down") == region {
            let destination = self.port_region(identity, seed, depth + 1, "up");
            out.push(StairConnection {
                id: format!("{identity}/{depth}"),
                from: format!("{region}/down"),
                direction: tor_world::Direction::Down,
                to: format!("{destination}/up"),
                destination,
            });
        }
        out
    }
}

/// Borrowed, immutable topology authority shared by compilation and streaming.
/// This helper owns address grammar and family lookup, never geometry or caches.
pub(crate) struct FamilyCatalog<'a> {
    families: &'a BTreeMap<String, WorldFamily>,
    seed: u64,
}
impl<'a> FamilyCatalog<'a> {
    pub(crate) fn new(families: &'a BTreeMap<String, WorldFamily>, seed: u64) -> Self {
        Self { families, seed }
    }
    pub(crate) fn find(&self, region: u64) -> Option<(&'a str, &'a WorldFamily)> {
        self.families
            .iter()
            .find(|(_, family)| family.address(region).is_some())
            .map(|(id, family)| (id.as_str(), family))
    }
    pub(crate) fn resolve_endpoint(&self, anchor: &str) -> Result<String, Failure> {
        let parts: Vec<_> = anchor.split('/').collect();
        match parts.as_slice() {
            [id, name] if id.parse::<u64>().is_ok() && !name.is_empty() => Ok(anchor.to_owned()),
            [identity, depth, port] => self
                .families
                .get(*identity)
                .ok_or_else(|| fail("Unknown procedural endpoint family"))?
                .resolve_port(identity, self.seed, depth, port),
            _ => Err(fail("Expected region/anchor or family/depth/port endpoint")),
        }
    }
    pub(crate) fn owner(&self, region: u64) -> Option<u64> {
        let (id, family) = self.find(region)?;
        let (depth, _) = family.address(region)?;
        (matches!(family.domain, Domain::Open { .. }) || depth <= family.depth_limit(id, self.seed))
            .then(|| family.owner(region))
            .flatten()
    }
    pub(crate) fn group_identity(&self, region: u64) -> Option<String> {
        let (id, family) = self.find(region)?;
        let (depth, slot) = family.address(region)?;
        (slot == 0 && self.owner(region).is_some()).then(|| format!("{id}/{depth}"))
    }
    pub(crate) fn materializable(&self, region: u64) -> bool {
        self.find(region)
            .is_none_or(|(id, family)| family.materializable(id, self.seed, region))
    }
    pub(crate) fn connections(&self, region: u64) -> Vec<StairConnection> {
        self.find(region).map_or_else(Vec::new, |(id, family)| {
            family.connections(id, self.seed, region)
        })
    }
}

/// Pure resolved structure. Destination coordinates are deliberately absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StairConnection {
    pub id: String,
    pub from: String,
    pub direction: tor_world::Direction,
    pub to: String,
    pub destination: u64,
}
