//! Structural preload planning, independent of entity construction and simulation.
//!
//! A plan is advisory: it does not freeze actors or unload regions. The streaming
//! owner must include all required body/effect regions among the roots before
//! using a plan, and commit a transition only at a consistent simulation boundary.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Serialize;
use tor_world::{Extent, Location, Position, RegionId};

use crate::{scenario_package::Package, Failure};

/// Immutable structural information available before generating region contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionMetadata {
    pub bounds: Extent,
    pub anchors: BTreeMap<String, Position>,
    pub zone: Option<String>,
    pub themes: BTreeSet<String>,
    pub outgoing: BTreeSet<RegionId>,
}

/// Backend-only index. Never disclose this catalog to a player client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionCatalog {
    regions: BTreeMap<RegionId, RegionMetadata>,
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
        let mut regions = BTreeMap::new();
        for region in &package.regions {
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
                zone: region.zone.clone(),
                themes,
                outgoing: BTreeSet::new(),
            };
            if region.id == 0 || regions.insert(RegionId(region.id), metadata).is_some() {
                return Err(invalid("Duplicate or zero structural region ID"));
            }
        }
        if regions.is_empty() {
            return Err(invalid("Empty region catalog"));
        }
        let mut catalog = Self { regions };
        for region in &package.regions {
            let mut outgoing = BTreeSet::new();
            for portal in &region.portals {
                outgoing.insert(catalog.resolve_anchor(&portal.to)?.region);
            }
            catalog
                .regions
                .get_mut(&RegionId(region.id))
                .expect("indexed region")
                .outgoing = outgoing;
        }
        Ok(catalog)
    }

    pub fn region(&self, id: RegionId) -> Option<&RegionMetadata> {
        self.regions.get(&id)
    }

    /// Resolve a fixed package anchor without requiring its region to be loaded.
    pub fn resolve_anchor(&self, anchor: &str) -> Result<Location, Failure> {
        let missing = || invalid(format!("Missing structural anchor {anchor}"));
        let (id, name) = anchor.split_once('/').ok_or_else(missing)?;
        let region = RegionId(id.parse().map_err(|_| missing())?);
        // Match the existing exact '<decimal region>/<name>' package namespace.
        if id != region.0.to_string() {
            return Err(missing());
        }
        let position = *self
            .regions
            .get(&region)
            .and_then(|r| r.anchors.get(name))
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
        for id in roots.iter().chain(active) {
            if !self.regions.contains_key(id) {
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
            for next in &self.regions[&id].outgoing {
                examined_links += 1;
                if required.insert(*next) {
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
