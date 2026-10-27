//! One chance and one stable room, then ordinary finite item/actor births.
//! Independent area-capped counts are an explicit TOR adaptation, not Rogue RNG parity.
use super::*;

fn requested(
    seed: u64,
    scope: &str,
    profile_id: &str,
    profile: &PopulationProfile,
    stage: &PopulationStage,
    count: [u16; 2],
    role: &str,
) -> u16 {
    let mut rng = stream(seed, scope, profile_id, profile, stage, 0, role);
    count[0] + index(&mut rng, usize::from(count[1] - count[0]) + 1) as u16
}

fn role_stage(stage: &PopulationStage, room: u32, count: u16, actors: bool) -> PopulationStage {
    let PopulationContent::RoomCluster {
        item_bands,
        actor_bands,
        avoid_protagonist_sight,
        ..
    } = &stage.population
    else {
        unreachable!()
    };
    PopulationStage {
        // An internal stream key, never authored or retained as a stage.
        // Authored labels reject control characters, so even an authored ID
        // resembling the readable suffix cannot alias a cluster role stream.
        id: format!(
            "\0{}/cluster/{room}/{}",
            stage.id,
            if actors { "actors" } else { "items" }
        ),
        version: stage.version,
        count: [count, count],
        population: if actors {
            PopulationContent::Actors {
                bands: actor_bands.clone(),
                avoid_protagonist_sight: *avoid_protagonist_sight,
            }
        } else {
            PopulationContent::Items {
                bands: item_bands.clone(),
            }
        },
    }
}

impl Game {
    pub(super) fn evaluate_room_cluster(
        &mut self,
        entrant: ActorId,
        scope: &PreparedScope,
        profile: &PopulationProfile,
        stage: &PopulationStage,
        newborn: &mut Vec<ActorId>,
    ) -> Result<StageResult, GameError> {
        let PopulationContent::RoomCluster {
            chance,
            item_count,
            actor_count,
            actor_depth_offset,
            ..
        } = &stage.population
        else {
            return Err(invalid());
        };
        let mut chance_rng = stream(
            self.seed,
            &scope.scope,
            &scope.profile,
            profile,
            stage,
            0,
            "cluster/chance",
        );
        let selection = if !chance.succeeds(&mut chance_rng) {
            ClusterSelection::ChanceMiss {}
        } else if scope.rooms.is_empty() {
            ClusterSelection::NoEligibleRoom {}
        } else {
            let mut room_rng = stream(
                self.seed,
                &scope.scope,
                &scope.profile,
                profile,
                stage,
                0,
                "cluster/room",
            );
            let room = &scope.rooms[index(&mut room_rng, scope.rooms.len())];
            let room_area = u16::try_from(room.cells.len()).map_err(|_| invalid())?;
            let item_requested = requested(
                self.seed,
                &scope.scope,
                &scope.profile,
                profile,
                stage,
                *item_count,
                "cluster/items/count",
            );
            let actor_requested = requested(
                self.seed,
                &scope.scope,
                &scope.profile,
                profile,
                stage,
                *actor_count,
                "cluster/actors/count",
            );
            let actor_depth = scope
                .depth
                .checked_add(*actor_depth_offset)
                .ok_or_else(invalid)?;
            let mut local = scope.clone();
            local.cells.clone_from(&room.cells);
            let items = self.evaluate_population_attempts(
                entrant,
                &local,
                profile,
                &role_stage(stage, room.id, item_requested.min(room_area), false),
                item_requested.min(room_area),
                newborn,
                true,
            )?;
            local.depth = actor_depth;
            let actors = self.evaluate_population_attempts(
                entrant,
                &local,
                profile,
                &role_stage(stage, room.id, actor_requested.min(room_area), true),
                actor_requested.min(room_area),
                newborn,
                true,
            )?;
            ClusterSelection::Selected {
                room: room.id,
                room_area,
                actor_depth,
                item_requested,
                actor_requested,
                items,
                actors,
            }
        };
        Ok(StageResult::RoomCluster { selection })
    }

    pub(super) fn validate_room_cluster(
        &self,
        record: &FloorActivationRecord,
        profile: &PopulationProfile,
        stage: &PopulationStage,
        depth: u32,
        selection: &ClusterSelection,
    ) -> Result<(), GameError> {
        let PopulationContent::RoomCluster {
            chance,
            item_count,
            actor_count,
            actor_depth_offset,
            ..
        } = &stage.population
        else {
            return Err(invalid());
        };
        let mut chance_rng = stream(
            self.seed,
            &record.scope,
            &record.profile,
            profile,
            stage,
            0,
            "cluster/chance",
        );
        let chance_hit = chance.succeeds(&mut chance_rng);
        let scope = self
            .floor_activation
            .as_ref()
            .and_then(|state| state.scopes.get(&record.scope));
        if scope
            .is_some_and(|scope| !stage.cluster_scope_valid(self.seed, profile, scope, selection))
        {
            return Err(invalid());
        }
        let (room_id, room_area, actor_depth, item_requested, actor_requested, items, actors) =
            match selection {
                ClusterSelection::ChanceMiss {} if !chance_hit => return Ok(()),
                ClusterSelection::NoEligibleRoom {}
                    if chance_hit && scope.is_none_or(|scope| scope.rooms.is_empty()) =>
                {
                    return Ok(())
                }
                ClusterSelection::Selected {
                    room,
                    room_area,
                    actor_depth,
                    item_requested,
                    actor_requested,
                    items,
                    actors,
                } if chance_hit => (
                    *room,
                    *room_area,
                    *actor_depth,
                    *item_requested,
                    *actor_requested,
                    items,
                    actors,
                ),
                _ => return Err(invalid()),
            };
        if room_id == 0
            || room_id > MAX_ROOM_GROUPS as u32
            || room_area == 0
            || usize::from(room_area) > MAX_ROOM_CELLS
            || Some(actor_depth) != depth.checked_add(*actor_depth_offset)
            || item_requested
                != requested(
                    self.seed,
                    &record.scope,
                    &record.profile,
                    profile,
                    stage,
                    *item_count,
                    "cluster/items/count",
                )
            || actor_requested
                != requested(
                    self.seed,
                    &record.scope,
                    &record.profile,
                    profile,
                    stage,
                    *actor_count,
                    "cluster/actors/count",
                )
            || items.len() != usize::from(item_requested.min(room_area))
            || actors.len() != usize::from(actor_requested.min(room_area))
        {
            return Err(invalid());
        }
        let selected = if let Some(scope) = scope {
            if scope.rooms.is_empty() {
                return Err(invalid());
            }
            let mut room_rng = stream(
                self.seed,
                &record.scope,
                &record.profile,
                profile,
                stage,
                0,
                "cluster/room",
            );
            let room = &scope.rooms[index(&mut room_rng, scope.rooms.len())];
            if room.id != room_id || room.cells.len() != usize::from(room_area) {
                return Err(invalid());
            }
            Some(room)
        } else {
            None
        };
        for (is_actor, attempts, requested, effective_depth) in [
            (false, items, item_requested, depth),
            (true, actors, actor_requested, actor_depth),
        ] {
            let role = role_stage(stage, room_id, requested.min(room_area), is_actor);
            self.validate_population_attempts(
                record,
                profile,
                &role,
                effective_depth,
                requested.min(room_area),
                attempts,
            )?;
            // Historical actors/items may be dead, consumed or elsewhere. Check only
            // immutable room and recipe facts, never present occupancy or ownership.
            for attempt in attempts {
                if let PlacementDecision::Placed { location, .. } = attempt {
                    if let (Some(scope), Some(room)) = (scope, selected) {
                        if room.cells.binary_search(location).is_err()
                            || scope.protected.contains(location)
                        {
                            return Err(invalid());
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Generated rooms describe z=0 floor columns in one slot's region, strictly
/// inside its margin. RecipeRun::room keeps x/y away from the unrotated boundary
/// portals authored by procedural_world. A footprint confined to these columns
/// therefore cannot cross a portal; local offsets are its immutable witness.
/// Live admission still checks actual 3D world body-fit. Recovery also checks
/// actual body cells against pinned generated terrain, never mutable occupancy.
pub(super) fn room_column_inside(
    cells: &[Location],
    protected: &BTreeSet<Location>,
    at: Location,
) -> bool {
    let floor = Location {
        position: tor_world::Position {
            z: 0,
            ..at.position
        },
        ..at
    };
    cells.binary_search(&floor).is_ok() && !protected.contains(&floor) && !protected.contains(&at)
}

fn local_body_valid(
    at: Location,
    body: &crate::BodySpec,
    allowed: impl Fn(Location) -> bool,
) -> bool {
    body.cells.iter().all(|offset| {
        let Some(x) = at.position.x.checked_add(offset[0]) else {
            return false;
        };
        let Some(y) = at.position.y.checked_add(offset[1]) else {
            return false;
        };
        let Some(z) = at.position.z.checked_add(offset[2]) else {
            return false;
        };
        allowed(Location {
            region: at.region,
            position: tor_world::Position { x, y, z },
        })
    })
}

pub(super) fn local_body_inside(
    cells: &[Location],
    protected: &BTreeSet<Location>,
    at: Location,
    body: &crate::BodySpec,
) -> bool {
    local_body_valid(at, body, |cell| room_column_inside(cells, protected, cell))
}

impl PopulationStage {
    /// Bind cluster geometry to the caller's immutable prepared scope. Servers
    /// reconstruct that scope from pinned inputs even for expired branches; no
    /// present entity, mutable terrain or current region residency is required.
    pub fn cluster_scope_valid(
        &self,
        seed: u64,
        profile: &PopulationProfile,
        scope: &PreparedScope,
        selection: &ClusterSelection,
    ) -> bool {
        let PopulationContent::RoomCluster {
            chance,
            actor_depth_offset,
            item_count,
            actor_count,
            ..
        } = &self.population
        else {
            return false;
        };
        if !scope.valid()
            || !chance.valid()
            || *actor_depth_offset > 1
            || [item_count, actor_count]
                .iter()
                .any(|count| count[0] == 0 || count[0] > count[1] || count[1] > MAX_ATTEMPTS)
            || self.maximum_attempts() > usize::from(MAX_ATTEMPTS)
        {
            return false;
        }
        let mut chance_rng = stream(
            seed,
            &scope.scope,
            &scope.profile,
            profile,
            self,
            0,
            "cluster/chance",
        );
        if !chance.succeeds(&mut chance_rng) {
            return matches!(selection, ClusterSelection::ChanceMiss {});
        }
        if scope.rooms.is_empty() {
            return matches!(selection, ClusterSelection::NoEligibleRoom {});
        }
        let ClusterSelection::Selected {
            room,
            room_area,
            actor_depth,
            item_requested,
            actor_requested,
            items,
            actors,
        } = selection
        else {
            return false;
        };
        let mut room_rng = stream(
            seed,
            &scope.scope,
            &scope.profile,
            profile,
            self,
            0,
            "cluster/room",
        );
        let selected = &scope.rooms[index(&mut room_rng, scope.rooms.len())];
        if *room != selected.id
            || usize::from(*room_area) != selected.cells.len()
            || Some(*actor_depth) != scope.depth.checked_add(*actor_depth_offset)
            || *item_requested
                != requested(
                    seed,
                    &scope.scope,
                    &scope.profile,
                    profile,
                    self,
                    *item_count,
                    "cluster/items/count",
                )
            || *actor_requested
                != requested(
                    seed,
                    &scope.scope,
                    &scope.profile,
                    profile,
                    self,
                    *actor_count,
                    "cluster/actors/count",
                )
            || items.len() != usize::from((*item_requested).min(*room_area))
            || actors.len() != usize::from((*actor_requested).min(*room_area))
        {
            return false;
        }
        if items.iter().chain(actors).any(|attempt| matches!(attempt,PlacementDecision::Placed { location, .. }
            if selected.cells.binary_search(location).is_err() || scope.protected.contains(location))) { return false; }
        self.cluster_body_geometry_valid(selection, |cell| {
            room_column_inside(&selected.cells, &scope.protected, cell)
        })
    }

    /// Check complete, checked local body offsets against immutable source
    /// geometry. Call alongside cluster_scope_valid: this deliberately does not
    /// duplicate chance/count/selected-room validation or inspect live entities.
    pub fn cluster_body_geometry_valid(
        &self,
        selection: &ClusterSelection,
        allowed: impl Fn(Location) -> bool,
    ) -> bool {
        let PopulationContent::RoomCluster { actor_bands, .. } = &self.population else {
            return false;
        };
        let ClusterSelection::Selected {
            actor_depth,
            actors,
            ..
        } = selection
        else {
            return true;
        };
        let Some(band) = eligible_band(actor_bands, *actor_depth) else {
            return false;
        };
        actors.iter().all(|attempt| match attempt {
            PlacementDecision::Skipped { .. } => true,
            PlacementDecision::Placed {
                location, content, ..
            } => band
                .choices
                .iter()
                .find(|choice| choice.content == *content)
                .is_some_and(|choice| {
                    local_body_valid(
                        *location,
                        &choice.actor.body.clone().unwrap_or_default(),
                        &allowed,
                    )
                }),
        })
    }
}
