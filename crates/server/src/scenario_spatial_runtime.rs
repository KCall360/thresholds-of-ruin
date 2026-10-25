//! Canonical source-to-World compiler and staged registry publication.
use super::spatial::{CanonicalSpatialSource, CompilerWorkBudget};
use super::*;
use tor_simulation::spatial_scope::{
    registry::{Budget, Registry, Scope},
    RoomId,
};

pub(super) enum JoinKind {
    Boundary,
    Portal,
    Transport,
}
pub(super) struct SourceJoin {
    pub passage: Passage,
    pub rotation: u8,
    pub width: u16,
    pub height: u16,
    pub kind: JoinKind,
}
impl SourceJoin {
    pub(super) fn apply_game(self, game: &mut Game) -> Result<(), tor_simulation::GameError> {
        match self.kind {
            JoinKind::Boundary => {
                game.connect_boundary_area(self.passage, self.rotation, self.width, self.height)
            }
            JoinKind::Portal => {
                game.connect_portal_area(self.passage, self.rotation, self.width, self.height)
            }
            JoinKind::Transport => {
                game.connect_area(self.passage, self.rotation, self.width, self.height)
            }
        }
    }
    fn apply_world(self, world: &mut World) -> Result<(), Failure> {
        match self.kind {
            JoinKind::Boundary => {
                world.connect_boundary_area(self.passage, self.rotation, self.width, self.height)
            }
            JoinKind::Portal => {
                world.connect_portal_area(self.passage, self.rotation, self.width, self.height)
            }
            JoinKind::Transport => {
                world.connect_area(self.passage, self.rotation, self.width, self.height)
            }
        }
        .map_err(|e| fail(format!("Spatial source join: {e:?}")))
    }
}
fn budget() -> Result<Budget, Failure> {
    tor_simulation::spatial_scope::codec::archive_structural_budget()
        .map_err(|_| fail("Spatial archive capacity"))
}
// Existing source validation work ceiling, independent of encoded registry charge.
fn work() -> CompilerWorkBudget {
    CompilerWorkBudget {
        source_entries: MAX_VALIDATION_CELL_WORK,
        terrain_cells: MAX_VALIDATION_CELL_WORK,
    }
}
fn charge(total: &mut u64, count: u64) -> Result<(), Failure> {
    *total = total
        .checked_add(count)
        .filter(|n| *n <= work().source_entries)
        .ok_or_else(|| fail("Spatial source work capacity"))?;
    Ok(())
}
fn length(first: i32, last: i32) -> Result<u64, Failure> {
    u64::try_from(i64::from(last) - i64::from(first) + 1)
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| fail("Invalid spatial inclusive extent"))
}
impl Package {
    fn spatial_world(
        &self,
        defs: &[&RegionDef],
        members: &BTreeSet<u64>,
        count: &mut u64,
    ) -> Result<World, Failure> {
        for def in defs {
            let extra = if def.chamber { 2u64 } else { 0 };
            let mut cells = 1u64;
            for side in def.size {
                cells = cells
                    .checked_mul(
                        u64::try_from(side)
                            .map_err(|_| fail("Spatial extent"))?
                            .checked_add(extra)
                            .ok_or_else(|| fail("Spatial extent"))?,
                    )
                    .ok_or_else(|| fail("Spatial extent"))?;
            }
            charge(count, cells)?;
        }
        let mut world = World::new(vec![], vec![]).map_err(|e| fail(format!("{e:?}")))?;
        for def in defs {
            let region = region_of(def.id, &def.name, def.size)?;
            (if def.chamber {
                world.add_chamber(region)
            } else {
                world.add_region(region)
            })
            .map_err(|e| fail(format!("Spatial region: {e:?}")))?;
            for at in &def.walls {
                world
                    .set_wall(loc(def.id, *at), true)
                    .map_err(|e| fail(format!("Spatial wall: {e:?}")))?;
            }
            for at in &def.openings {
                world
                    .set_wall(loc(def.id, *at), false)
                    .map_err(|e| fail(format!("Spatial opening: {e:?}")))?;
            }
        }
        let anchors = resolved_anchors(defs);
        for def in defs.iter().filter(|d| members.contains(&d.id)) {
            for join in self.spatial_joins(def, &anchors)? {
                join.apply_world(&mut world)?;
            }
        }
        Ok(world)
    }
    /// Same bounded destination-shell convention as build_region. Member
    /// geometry is retained from the caller's actual source/generation pass;
    /// only outgoing destinations are resolved. Neighbor links are not followed.
    fn spatial_source_world(
        &self,
        seed: u64,
        defs: &[&RegionDef],
        terrain: &mut u64,
    ) -> Result<World, Failure> {
        let members: BTreeSet<_> = defs.iter().map(|d| d.id).collect();
        let mut external = BTreeSet::new();
        for def in defs {
            for portal in &def.portals {
                let destination = self.endpoint_region_seed(seed, &portal.to)?;
                if !members.contains(&destination) {
                    external.insert(destination);
                }
            }
        }
        let mut neighbors = Vec::new();
        if !external.is_empty() {
            let index = self.index(seed)?;
            for id in external {
                neighbors.push(index.region(self, id)?);
            }
        }
        let shell: Vec<_> = defs
            .iter()
            .copied()
            .chain(neighbors.iter().map(|d| &**d))
            .collect();
        self.spatial_world(&shell, &members, terrain)
    }
    fn authored_spatial_source(
        &self,
        id: &str,
        definition: &spatial_authoring::AuthoredSpatialScope,
        defs: &[&RegionDef],
        world: &World,
        entries: &mut u64,
    ) -> Result<CanonicalSpatialSource, Failure> {
        require(label(id), "Invalid spatial scope identity")?;
        let mut members: Vec<_> = definition.members.iter().copied().map(RegionId).collect();
        members.sort();
        require(
            !members.is_empty() && members.windows(2).all(|w| w[0] != w[1]),
            "Duplicate or empty spatial members",
        )?;
        let contained = |region: u64, point: [i32; 3]| -> Result<(), Failure> {
            require(
                members.binary_search(&RegionId(region)).is_ok()
                    && world.contains(loc(region, point)),
                "Spatial source extent outside member",
            )
        };
        for volumes in definition.rooms.values() {
            for volume in volumes {
                contained(volume.region, volume.min)?;
                contained(volume.region, volume.max)?;
                let mut count = 1u64;
                for axis in 0..3 {
                    count = count
                        .checked_mul(length(volume.min[axis], volume.max[axis])?)
                        .ok_or_else(|| fail("Spatial volume overflow"))?;
                }
                charge(entries, count)?;
            }
        }
        for run in &definition.passages {
            contained(run.region, [run.first_x, run.y, run.z])?;
            contained(run.region, [run.last_x, run.y, run.z])?;
            charge(entries, length(run.first_x, run.last_x)?)?;
        }
        for run in &definition.doorways {
            contained(run.region, [run.first_x, run.y, run.z])?;
            contained(run.region, [run.last_x, run.y, run.z])?;
            charge(entries, length(run.first_x, run.last_x)?)?;
        }
        let mut rooms = BTreeMap::new();
        for (id, volumes) in &definition.rooms {
            let room = RoomId::new(*id).ok_or_else(|| fail("Invalid spatial room identity"))?;
            let mut cells = BTreeSet::new();
            for volume in volumes {
                for z in volume.min[2]..=volume.max[2] {
                    for y in volume.min[1]..=volume.max[1] {
                        for x in volume.min[0]..=volume.max[0] {
                            cells.insert(loc(volume.region, [x, y, z]));
                        }
                    }
                }
            }
            rooms.insert(room, cells.into_iter().collect());
        }
        let mut passages = Vec::new();
        for run in &definition.passages {
            for x in run.first_x..=run.last_x {
                passages.push(loc(run.region, [x, run.y, run.z]));
            }
        }
        let mut doorways = BTreeMap::new();
        for run in &definition.doorways {
            let room = RoomId::new(run.room).ok_or_else(|| fail("Invalid doorway room"))?;
            for x in run.first_x..=run.last_x {
                require(
                    doorways
                        .insert(loc(run.region, [x, run.y, run.z]), room)
                        .is_none(),
                    "Duplicate spatial doorway",
                )?;
            }
        }
        let mut closure_faces = BTreeSet::new();
        for def in defs {
            for door in &def.doors {
                for dz in 0..i32::from(door.height) {
                    let z = door.at[2]
                        .checked_add(dz)
                        .ok_or_else(|| fail("Door closure overflow"))?;
                    let at = loc(def.id, [door.at[0], door.at[1], z]);
                    if doorways.contains_key(&at) {
                        charge(entries, 1)?;
                        closure_faces.insert(at);
                    }
                }
            }
        }
        Ok(CanonicalSpatialSource {
            id: id.into(),
            members,
            rooms,
            passages,
            doorways,
            closure_faces,
        })
    }
    pub(crate) fn validate_spatial_sources(&self) -> Result<(), Failure> {
        let mut owners = BTreeSet::new();
        let mut source_entries = 0;
        let mut terrain = 0;
        for (id, definition) in &self.manifest.spatial_scopes {
            for member in &definition.members {
                require(
                    owners.insert(*member),
                    "Region belongs to multiple spatial scopes",
                )?;
                require(
                    self.index.region(*member).is_some_and(|r| !r.generated),
                    "Authored spatial member must be authored",
                )?;
            }
            // Validate future source geometry, but do not retain it or charge
            // unprepared scopes against a runtime archive directory.
            self.compile_authored_spatial(id, 0, &mut source_entries, &mut terrain)?;
        }
        if spatial_sources::spatial_effect_enabled(self)? {
            let generated: BTreeSet<_> = self
                .manifest
                .generation_groups
                .values()
                .flat_map(|g| g.members.iter().copied())
                .collect();
            for region in &self.index.regions {
                require(
                    generated.contains(&region.id) || owners.contains(&region.id),
                    "Enabled spatial effect requires canonical spatial scope for every authored region",
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn compile_authored_spatial(
        &self,
        id: &str,
        seed: u64,
        source_entries: &mut u64,
        terrain: &mut u64,
    ) -> Result<Scope, Failure> {
        let definition = self
            .manifest
            .spatial_scopes
            .get(id)
            .ok_or_else(|| fail("Unknown authored spatial scope"))?;
        let index = self.index(seed)?;
        let definitions: Vec<_> = definition
            .members
            .iter()
            .map(|id| index.region(self, *id))
            .collect::<Result<_, _>>()?;
        let refs: Vec<_> = definitions.iter().map(|d| &**d).collect();
        let world = self.spatial_source_world(seed, &refs, terrain)?;
        let source = self.authored_spatial_source(id, definition, &refs, &world, source_entries)?;
        spatial::compile(&world, &source, &budget()?, &work())
    }
    pub(crate) fn canonical_generated_spatial(
        &self,
        id: &str,
        seed: u64,
        generated: &crate::generation_recipe::GeneratedGroup,
    ) -> Result<Scope, Failure> {
        require(
            !self.manifest.spatial_scopes.contains_key(id),
            "Generated/authored spatial identity conflict",
        )?;
        let group = self.generation_group(id, seed)?;
        let budget = budget()?;
        let source = spatial::source_from_generated(id, &group, generated, &budget, &work())?;
        let defs: Vec<_> = generated.regions.values().collect();
        let world = self.spatial_source_world(seed, &defs, &mut 0)?;
        spatial::compile(&world, &source, &budget, &work())
    }
    pub(crate) fn install_authored_spatial(&self, game: &mut Game) -> Result<(), Failure> {
        let realized: BTreeSet<_> = game
            .loaded_regions()
            .chain(game.detached_records().map(|(r, _)| r))
            .collect();
        let installed: BTreeSet<_> = game
            .spatial_registry()
            .scopes()
            .map(|s| s.id.as_str())
            .collect();
        if !self.manifest.spatial_scopes.iter().any(|(id, s)| {
            !installed.contains(id.as_str())
                && s.members.iter().any(|r| realized.contains(&RegionId(*r)))
        }) {
            return Ok(());
        }
        let mut next = game.clone();
        let budget = budget()?;
        let mut source_entries = 0;
        let mut terrain = 0;
        for (id, definition) in &self.manifest.spatial_scopes {
            if !installed.contains(id.as_str())
                && definition
                    .members
                    .iter()
                    .any(|r| realized.contains(&RegionId(*r)))
            {
                let scope = self.compile_authored_spatial(
                    id,
                    game.seed(),
                    &mut source_entries,
                    &mut terrain,
                )?;
                next.register_spatial_scope(scope, &budget)
                    .map_err(|e| fail(format!("Spatial registration: {e:?}")))?;
            }
        }
        *game = next;
        Ok(())
    }
    pub(crate) fn install_spatial_scope(
        &self,
        game: &mut Game,
        scope: Scope,
    ) -> Result<(), Failure> {
        game.register_spatial_scope(scope, &budget()?)
            .map_err(|e| fail(format!("Spatial registration: {e:?}")))
    }
}

/// Per-checkpoint source authority. Payload recompilation is shared, but expected
/// directory completeness is checked independently for EVERY restored boundary.
pub(crate) struct SpatialAudit<'a> {
    package: Option<&'a Package>,
    seed: u64,
    index: Option<PackageIndex>,
    canonical: Registry,
}
impl<'a> SpatialAudit<'a> {
    pub(crate) fn new(package: Option<&'a Package>, seed: u64) -> Result<Self, Failure> {
        let index = match package {
            Some(p) => {
                p.validate_spatial_sources()?;
                Some(p.index(seed)?)
            }
            None => None,
        };
        Ok(Self {
            package,
            seed,
            index,
            canonical: Registry::default(),
        })
    }
    pub(crate) fn check(&mut self, game: &Game) -> Result<(), Failure> {
        let invalid = || {
            Failure::new(
                tor_protocol::ErrorCode::InvalidArchive,
                "Spatial registry differs from canonical source",
            )
        };
        let Some(package) = self.package else {
            return if game.spatial_registry().scopes().next().is_none() {
                Ok(())
            } else {
                Err(Failure::new(
                    tor_protocol::ErrorCode::InvalidArchive,
                    "Spatial registry requires canonical source verification",
                ))
            };
        };
        require(game.seed() == self.seed, "Spatial source seed mismatch").map_err(|_| invalid())?;
        let realized: BTreeSet<_> = game
            .loaded_regions()
            .chain(game.detached_records().map(|(r, _)| r))
            .collect();
        let authored: BTreeSet<_> = package
            .manifest
            .spatial_scopes
            .iter()
            .filter(|(_, s)| s.members.iter().any(|r| realized.contains(&RegionId(*r))))
            .map(|(id, _)| id.clone())
            .collect();
        let index = self.index.as_ref().ok_or_else(invalid)?;
        let generated: BTreeSet<_> = realized
            .iter()
            .filter_map(|r| index.group_owner(r.0))
            .map(|owner| index.group_identity(owner).ok_or_else(invalid))
            .collect::<Result<_, _>>()?;
        let mut expected = authored.clone();
        let mut source_entries = 0;
        let mut terrain = 0;
        for id in &authored {
            if !self.canonical.scopes().any(|s| s.id == *id) {
                let scope = package
                    .compile_authored_spatial(id, self.seed, &mut source_entries, &mut terrain)
                    .map_err(|_| invalid())?;
                self.canonical
                    .register_archive_checked(scope)
                    .map_err(|_| invalid())?;
            }
        }
        for id in &generated {
            if !expected.insert(id.clone()) {
                return Err(invalid());
            }
            let group = package
                .generation_group(id, self.seed)
                .map_err(|_| invalid())?;
            if !group
                .members
                .iter()
                .all(|r| realized.contains(&RegionId(*r)))
            {
                return Err(invalid());
            }
            if !self.canonical.scopes().any(|s| s.id == *id) {
                let generated = package
                    .generated_group(id, self.seed)
                    .map_err(|_| invalid())?;
                let scope = package
                    .canonical_generated_spatial(id, self.seed, &generated)
                    .map_err(|_| invalid())?;
                self.canonical
                    .register_archive_checked(scope)
                    .map_err(|_| invalid())?;
            }
        }
        if game.spatial_registry().scopes().count() != expected.len() {
            return Err(invalid());
        }
        for scope in game.spatial_registry().scopes() {
            if !expected.contains(&scope.id)
                || self.canonical.scopes().find(|s| s.id == scope.id) != Some(scope)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "scenario_spatial_runtime_tests.rs"]
pub(crate) mod tests;
