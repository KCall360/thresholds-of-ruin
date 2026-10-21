//! Structural preload planning, independent of entity construction and simulation.
//!
//! A plan is advisory: it does not freeze actors or unload regions. The streaming
//! owner must include all required body/effect regions among the roots before
//! using a plan, and commit a transition only at a consistent simulation boundary.
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Serialize;
use tor_world::{Extent, Location, Position, RegionId};

use crate::{scenario_package::Package, Failure};

/// Immutable structural information available before generating region contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionMetadata {
    pub bounds: Extent,
    pub anchors: BTreeMap<String, Position>,
    pub stair_anchors: BTreeSet<String>,
    pub zone: Option<String>,
    pub themes: BTreeSet<String>,
    pub outgoing: BTreeSet<RegionId>,
}

/// Backend-only index. Never disclose this catalog to a player client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionCatalog {
    regions: BTreeMap<RegionId, RegionMetadata>,
    families: BTreeMap<String, crate::procedural_world::WorldFamily>,
    family_themes: BTreeMap<String, BTreeSet<String>>,
    seed: u64,
    links: BTreeMap<RegionId, BTreeSet<RegionId>>,
}

/// Sorted, deterministic transition candidates with explicit query work counts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HorizonPlan {
    pub required: BTreeSet<RegionId>,
    pub activate: BTreeSet<RegionId>,
    pub deactivate: BTreeSet<RegionId>,
    pub expanded_regions: usize,
    pub examined_links: usize,
}

fn invalid(message: impl AsRef<str>) -> Failure {
    Failure::new(tor_protocol::ErrorCode::InvalidAction, message.as_ref())
}

impl RegionCatalog {
    /// Build once from pinned package metadata. This does not validate gameplay
    /// geometry, generate contents, inspect entity placements, or consume RNG.
    pub fn from_package(package: &Package) -> Result<Self, Failure> {
        Self::from_package_seed(package, 0)
    }
    pub fn from_package_seed(package: &Package, seed: u64) -> Result<Self, Failure> {
        let mut families = BTreeMap::new();
        let mut family_themes = BTreeMap::new();
        for (id, family) in &package.manifest.procedural_worlds {
            family.check()?;
            let themes = family
                .zone
                .as_ref()
                .and_then(|zone| package.manifest.zones.get(zone))
                .and_then(|zone| zone.themes.as_ref())
                .unwrap_or(&package.manifest.themes)
                .iter()
                .cloned()
                .collect();
            families.insert(id.clone(), family.clone());
            family_themes.insert(id.clone(), themes);
        }
        let mut regions = BTreeMap::new();
        for region in &package.index.regions {
            let generated: BTreeSet<_> = region.stair_anchors.iter().collect();
            if generated.len() != region.stair_anchors.len()
                || (!region.generated && !generated.is_empty())
                || generated.iter().any(|name| {
                    name.is_empty()
                        || name.len() > 80
                        || name.contains('/')
                        || name.chars().any(char::is_control)
                        || region.anchors.contains_key(*name)
                })
            {
                return Err(invalid(format!(
                    "Region {}: invalid generated stair anchors",
                    region.id
                )));
            }
            let bounds =
                Extent::new(region.size[0], region.size[1], region.size[2]).ok_or_else(|| {
                    invalid(format!("Region {}: invalid structural bounds", region.id))
                })?;
            let anchors = region
                .anchors
                .iter()
                .map(|(name, &[x, y, z])| {
                    let position = Position { x, y, z };
                    if name.is_empty() || name.contains('/') || !bounds.contains(position) {
                        return Err(invalid(format!(
                            "Region {}: invalid structural anchor {name}",
                            region.id
                        )));
                    }
                    Ok((name.clone(), position))
                })
                .collect::<Result<_, _>>()?;
            let themes = match &region.zone {
                Some(zone) => package
                    .manifest
                    .zones
                    .get(zone)
                    .ok_or_else(|| invalid(format!("Region {}: unknown zone {zone}", region.id)))?
                    .themes
                    .as_ref()
                    .unwrap_or(&package.manifest.themes),
                None => &package.manifest.themes,
            }
            .iter()
            .cloned()
            .collect();
            let metadata = RegionMetadata {
                bounds,
                anchors,
                stair_anchors: region.stair_anchors.iter().cloned().collect(),
                zone: region.zone.clone(),
                themes,
                outgoing: BTreeSet::new(),
            };
            if region.id == 0 || regions.insert(RegionId(region.id), metadata).is_some() {
                return Err(invalid("Duplicate or zero structural region ID"));
            }
        }
        if regions.is_empty() && families.is_empty() {
            return Err(invalid("Empty region catalog"));
        }
        let mut catalog = Self {
            regions,
            families,
            family_themes,
            seed,
            links: BTreeMap::new(),
        };
        for region in &package.index.regions {
            let mut outgoing = BTreeSet::new();
            for to in &region.portals {
                outgoing.insert(catalog.anchor_region(to)?);
            }
            catalog
                .regions
                .get_mut(&RegionId(region.id))
                .expect("indexed region")
                .outgoing = outgoing;
        }
        for (id, pair) in &package.manifest.stair_pairs {
            if id.is_empty()
                || id.len() > 80
                || id.contains('/')
                || id.chars().any(char::is_control)
            {
                return Err(invalid(format!("Invalid stair pair ID {id:?}")));
            }
            let upper = catalog.anchor_region(&pair.upper)?;
            let lower = catalog.anchor_region(&pair.lower)?;
            catalog.links.entry(upper).or_default().insert(lower);
            catalog.links.entry(lower).or_default().insert(upper);
        }
        Ok(catalog)
    }

    fn family_catalog(&self) -> crate::procedural_world::FamilyCatalog<'_> {
        crate::procedural_world::FamilyCatalog::new(&self.families, self.seed)
    }
    fn resolve_endpoint(&self, anchor: &str) -> Result<String, Failure> {
        self.family_catalog().resolve_endpoint(anchor)
    }
    pub(crate) fn materializable(&self, region: RegionId) -> bool {
        self.family_catalog().materializable(region.0)
    }
    /// Region identity is known even when generation has not placed an anchor.
    pub fn anchor_region(&self, anchor: &str) -> Result<RegionId, Failure> {
        let anchor = self.resolve_endpoint(anchor)?;
        let missing = || invalid(format!("Missing structural anchor {anchor}"));
        let (id, name) = anchor.split_once('/').ok_or_else(missing)?;
        let region = RegionId(id.parse().map_err(|_| missing())?);
        if id != region.0.to_string()
            || !self
                .region(region)
                .is_some_and(|r| r.anchors.contains_key(name) || r.stair_anchors.contains(name))
        {
            return Err(missing());
        }
        Ok(region)
    }

    pub fn region(&self, id: RegionId) -> Option<Cow<'_, RegionMetadata>> {
        if let Some(metadata) = self.regions.get(&id) {
            if let Some(links) = self.links.get(&id) {
                let mut metadata = metadata.clone();
                metadata.outgoing.extend(links);
                return Some(Cow::Owned(metadata));
            }
            return Some(Cow::Borrowed(metadata));
        }
        let (identity, family) = self.family_catalog().find(id.0)?;
        let def = family.describe(identity, self.seed, id.0).ok()??;
        let mut outgoing: BTreeSet<_> = def
            .portals
            .iter()
            .map(|portal| {
                RegionId(
                    portal
                        .to
                        .split_once('/')
                        .expect("descriptor endpoint")
                        .0
                        .parse()
                        .expect("descriptor region"),
                )
            })
            .collect();
        outgoing.extend(
            self.family_catalog()
                .connections(id.0)
                .into_iter()
                .map(|exit| RegionId(exit.destination)),
        );
        outgoing.extend(self.links.get(&id).into_iter().flatten());
        Some(Cow::Owned(RegionMetadata {
            bounds: Extent::new(def.size[0], def.size[1], def.size[2])?,
            anchors: def
                .anchors
                .into_iter()
                .map(|(name, [x, y, z])| (name, Position { x, y, z }))
                .collect(),
            stair_anchors: def.generate?.stair_anchors.into_iter().collect(),
            zone: def.zone,
            themes: self
                .family_themes
                .get(identity)
                .cloned()
                .unwrap_or_default(),
            outgoing,
        }))
    }

    /// Resolve a fixed package anchor without requiring its region to be loaded.
    pub fn resolve_anchor(&self, anchor: &str) -> Result<Location, Failure> {
        let anchor = self.resolve_endpoint(anchor)?;
        let missing = || invalid(format!("Missing structural anchor {anchor}"));
        let (id, name) = anchor.split_once('/').ok_or_else(missing)?;
        let region = RegionId(id.parse().map_err(|_| missing())?);
        // Match the existing exact '<decimal region>/<name>' package namespace.
        if id != region.0.to_string() {
            return Err(missing());
        }
        let position = self
            .region(region)
            .and_then(|r| r.anchors.get(name).copied())
            .ok_or_else(missing)?;
        Ok(Location { region, position })
    }

    /// Union of roots and regions reachable through at most `hops` outgoing
    /// links. Doors, occupants, and runtime walkability do not affect preloading.
    /// Incoming links are not implicitly reversible. Cycles terminate even for
    /// an unbounded hop count. Only the reached neighborhood is expanded.
    pub fn plan(
        &self,
        roots: &BTreeSet<RegionId>,
        hops: usize,
        active: &BTreeSet<RegionId>,
    ) -> Result<HorizonPlan, Failure> {
        if roots.is_empty() {
            return Err(invalid(
                "A preload horizon requires at least one root region",
            ));
        }
        const MAX_QUERY_REGIONS: usize = 16_384;
        const MAX_QUERY_LINKS: usize = 131_072;
        let bounded = !self.families.is_empty();
        if bounded && (roots.len() > MAX_QUERY_REGIONS || active.len() > MAX_QUERY_REGIONS) {
            return Err(invalid(
                "Procedural horizon exceeds structural query budget",
            ));
        }
        for id in roots.iter().chain(active) {
            if !self.materializable(*id) {
                return Err(invalid("Procedural materialization depth limit reached"));
            }
            if self.region(*id).is_none() {
                return Err(invalid(format!("Unknown horizon region {}", id.0)));
            }
        }
        let mut required = roots.clone();
        let mut queue: VecDeque<_> = roots.iter().copied().map(|id| (id, 0)).collect();
        let mut expanded_regions = 0;
        let mut examined_links = 0;
        while let Some((id, distance)) = queue.pop_front() {
            if distance == hops {
                continue;
            }
            expanded_regions += 1;
            // Finite authored graphs retain cycle-terminating unbounded-hop queries.
            // Procedural graphs always enforce a bounded structural work budget.
            if !self.families.is_empty() && (expanded_regions > 16_384 || required.len() > 16_384) {
                return Err(invalid(
                    "Procedural horizon exceeds structural query budget",
                ));
            }
            let metadata = self.region(id).ok_or_else(|| {
                invalid("Procedural horizon exceeds domain or operational depth limit")
            })?;
            for next in &metadata.outgoing {
                if bounded && examined_links == MAX_QUERY_LINKS {
                    return Err(invalid("Procedural horizon exceeds structural link budget"));
                }
                examined_links += 1;
                // Keep the declared edge private and intact, but do not demand
                // terrain outside an explicit operational generation budget.
                if !self.materializable(*next) {
                    continue;
                }
                if !required.contains(next) {
                    if bounded && required.len() == MAX_QUERY_REGIONS {
                        return Err(invalid(
                            "Procedural horizon exceeds structural query budget",
                        ));
                    }
                    required.insert(*next);
                    queue.push_back((*next, distance + 1));
                }
            }
        }
        Ok(HorizonPlan {
            activate: required.difference(active).copied().collect(),
            deactivate: active.difference(&required).copied().collect(),
            required,
            expanded_regions,
            examined_links,
        })
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    fn metadata() -> RegionMetadata {
        RegionMetadata {
            bounds: Extent::new(5, 5, 2).unwrap(),
            anchors: BTreeMap::new(),
            stair_anchors: BTreeSet::new(),
            zone: None,
            themes: BTreeSet::new(),
            outgoing: BTreeSet::new(),
        }
    }
    fn catalog() -> RegionCatalog {
        RegionCatalog {
            regions: BTreeMap::from([(RegionId(1), metadata())]),
            families: BTreeMap::from([(
                "bounded".into(),
                crate::procedural_world::WorldFamily {
                    kind: crate::procedural_world::FamilyKind::GridChain,
                    version: 1,
                    base_region: 1_000_000,
                    domain: crate::procedural_world::Domain::Finite { count: 1 },
                    grid: [1, 1],
                    size: [5, 5, 2],
                    recipe: "rooms".into(),
                    gravity: None,
                    zone: None,
                },
            )]),
            family_themes: BTreeMap::new(),
            seed: 0,
            links: BTreeMap::new(),
        }
    }
    #[test]
    fn zero_hop_roots_and_active_sets_cannot_bypass_query_budget() {
        let catalog = catalog();
        let many = (1..=16_385).map(RegionId).collect();
        assert!(catalog.plan(&many, 0, &BTreeSet::new()).is_err());
        assert!(catalog
            .plan(&BTreeSet::from([RegionId(1)]), 0, &many)
            .is_err());
    }
    #[test]
    fn wide_shallow_authored_hub_cannot_bypass_procedural_query_budget() {
        let mut catalog = catalog();
        catalog.regions.get_mut(&RegionId(1)).unwrap().outgoing =
            (2..=16_386).map(RegionId).collect();
        for id in 2..=16_386 {
            catalog.regions.insert(RegionId(id), metadata());
        }
        assert!(catalog
            .plan(&BTreeSet::from([RegionId(1)]), 1, &BTreeSet::new())
            .is_err());
    }
}
