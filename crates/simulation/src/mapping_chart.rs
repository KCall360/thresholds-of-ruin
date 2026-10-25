//! Sparse private topology acquisition. Never sight, navigation, or explored-map mutation.
use crate::chart::{self, Candidate, Component, Kind, MappingMark, Mark, Row, Unavailable};
use crate::detection::CensusBudget;
use crate::spatial_scope::{mapping, registry::Scope, PrimarySpace, RoomId};
use crate::{ActorId, Game, GameError};
use tor_world::{Direction, Location, Position, World};

/// Private source witness. A wall uses an owned member anchor, NOT its possibly
/// foreign solid neighbor. Neither room nor anchor/toward belongs on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SourceMark {
    WallFace { room: RoomId, toward: Direction },
    Passage,
    Doorway,
    Stairs,
    SecretDoor,
    TrapPresence,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SourceRow {
    pub location: Location,
    pub mark: SourceMark,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedMapping {
    pub candidate: Candidate,
    pub scope: Option<String>,
    pub source_rows: Vec<SourceRow>,
}
impl PreparedMapping {
    fn unavailable(reason: Unavailable) -> Self {
        Self {
            candidate: Candidate::unavailable(reason),
            scope: None,
            source_rows: vec![],
        }
    }
}
fn pay(left: &mut usize, amount: usize) -> Result<(), Unavailable> {
    *left = left.checked_sub(amount).ok_or(Unavailable::Budget)?;
    Ok(())
}
fn reserve<T>(count: usize, budget: &mut CensusBudget) -> Result<Vec<T>, Unavailable> {
    pay(&mut budget.work, count)?;
    pay(
        &mut budget.allocation_bytes,
        count
            .checked_mul(std::mem::size_of::<T>())
            .ok_or(Unavailable::Budget)?,
    )?;
    let mut out = Vec::new();
    out.try_reserve_exact(count)
        .map_err(|_| Unavailable::Budget)?;
    Ok(out)
}
fn sorting(count: usize, budget: &mut CensusBudget) -> Result<(), Unavailable> {
    let log = usize::BITS as usize - count.leading_zeros() as usize + 1;
    pay(
        &mut budget.work,
        count
            .checked_mul(log)
            .and_then(|n| n.checked_mul(16))
            .ok_or(Unavailable::Budget)?,
    )
}
/// No room-interior traversal. Sparse source primary corridors and feature maps
/// are counted before allocation; every repeated enumeration is independently paid.
fn visit_sparse(
    world: &World,
    scope: &Scope,
    budget: &mut CensusBudget,
    mut emit: impl FnMut(SourceRow) -> Result<(), Unavailable>,
) -> Result<(), Unavailable> {
    for run in &scope.primary {
        pay(&mut budget.work, 1)?;
        let passage = matches!(run.membership.primary(), PrimarySpace::Passage(_));
        let doorway = run.membership.doorway().is_some();
        if !passage && !doorway {
            continue;
        }
        for x in run.cells.first_x..=run.cells.last_x {
            pay(&mut budget.work, 1)?;
            let at = Location {
                region: run.cells.region,
                position: Position {
                    x,
                    y: run.cells.y,
                    z: run.cells.z,
                },
            };
            if !world.contains(at) {
                return Err(Unavailable::Projection);
            }
            if world.is_wall(at) {
                continue;
            }
            if passage {
                emit(SourceRow {
                    location: at,
                    mark: SourceMark::Passage,
                })?;
            }
            if doorway {
                emit(SourceRow {
                    location: at,
                    mark: SourceMark::Doorway,
                })?;
            }
        }
    }
    for region in &scope.members {
        // Five sparse range starts, conservatively covering logarithmic lookup.
        pay(&mut budget.work, 5 * usize::BITS as usize)?;
        for at in world.doors_in_region(*region) {
            pay(&mut budget.work, 1)?;
            emit(SourceRow {
                location: at,
                mark: SourceMark::Doorway,
            })?;
        }
        for (at, stair) in world.stair_candidates_in_region(*region) {
            pay(&mut budget.work, usize::BITS as usize)?;
            if stair {
                emit(SourceRow {
                    location: at,
                    mark: SourceMark::Stairs,
                })?;
            }
        }
        for (at, _, _) in world.secret_doors_in_region(*region) {
            pay(&mut budget.work, 1)?;
            emit(SourceRow {
                location: at,
                mark: SourceMark::SecretDoor,
            })?;
        }
        for (at, _) in world.traps_in_region(*region) {
            pay(&mut budget.work, 1)?;
            emit(SourceRow {
                location: at,
                mark: SourceMark::TrapPresence,
            })?;
        }
    }
    Ok(())
}
/// Complete pure replay boundary for historical validation. It uses captured
/// World geometry, not original source terrain or later live feature locations.
pub(crate) fn prepare_scope_mapping(
    world: &World,
    scope: &Scope,
    budget: CensusBudget,
) -> PreparedMapping {
    let build = || -> Result<PreparedMapping, Unavailable> {
        let mut budget = budget;
        if !matches!(scope.mapping, mapping::MappingCertification::Certified(_)) {
            return Err(match scope.mapping {
                mapping::MappingCertification::CapacityUnavailable => Unavailable::Budget,
                _ => Unavailable::Projection,
            });
        }
        for region in &scope.members {
            pay(&mut budget.work, 1)?;
            if world.region(*region).is_none() {
                return Err(Unavailable::Projection);
            }
        }
        // Foreign shell is geometry-only: require it resident, never include it
        // in the exclusive projection/census component membership.
        for room in &scope.rooms {
            for run in &room.runs {
                pay(&mut budget.work, 1)?;
                if world.region(run.region).is_none() {
                    return Err(Unavailable::Projection);
                }
            }
        }
        for region in &scope.members {
            for link in world.exits(*region) {
                pay(&mut budget.work, usize::BITS as usize)?;
                if !world.is_stair(link.from, link.direction)
                    && world.region(link.to.region).is_none()
                {
                    return Err(Unavailable::Projection);
                }
            }
        }
        let faces = mapping::wall_faces_with_usage(
            world,
            scope,
            mapping::FaceBudget {
                work: budget.work,
                allocation_bytes: budget.allocation_bytes,
                faces: budget.allocation_bytes / std::mem::size_of::<mapping::MappingFace>(),
            },
        )
        .map_err(|why| match why {
            mapping::MappingUnavailable::Capacity => Unavailable::Budget,
            _ => Unavailable::Projection,
        })?;
        pay(&mut budget.work, faces.work)?;
        pay(&mut budget.allocation_bytes, faces.allocation_bytes)?;
        let mut count = faces.faces.len();
        visit_sparse(world, scope, &mut budget, |_| {
            count = count.checked_add(1).ok_or(Unavailable::Budget)?;
            Ok(())
        })?;
        let mut sources = reserve::<SourceRow>(count, &mut budget)?;
        for face in faces.faces {
            sources.push(SourceRow {
                location: face.anchor,
                mark: SourceMark::WallFace {
                    room: face.room,
                    toward: face.toward,
                },
            });
        }
        visit_sparse(world, scope, &mut budget, |row| {
            sources.push(row);
            Ok(())
        })?;
        sorting(sources.len(), &mut budget)?;
        sources.sort_unstable();
        pay(&mut budget.work, sources.len())?;
        // Presence is one fact at an actual source point, not a count of stair
        // destinations. Wall occurrences retain distinct room/anchor/toward.
        sources.dedup();
        let mut points = reserve::<Location>(sources.len(), &mut budget)?;
        points.extend(sources.iter().map(|row| row.location));
        let (projection, orientations) = crate::scope_projection::project_oriented(
            world,
            &scope.members,
            &points,
            crate::scope_projection::Budget {
                work: budget.work,
                allocation_bytes: budget.allocation_bytes,
                regions: scope.members.len(),
                directed_constraints: budget.work,
                samples: points.len(),
            },
        )
        .map_err(|error| match error {
            crate::scope_projection::Error::InvalidSource => Unavailable::Projection,
            _ => Unavailable::Budget,
        })?;
        pay(&mut budget.work, projection.usage.work)?;
        pay(
            &mut budget.allocation_bytes,
            projection.usage.allocation_bytes,
        )?;
        let mut ordered = reserve::<(Row, SourceRow)>(sources.len(), &mut budget)?;
        for ((source, projected), orientation) in sources
            .into_iter()
            .zip(projection.samples)
            .zip(orientations)
        {
            let mut position = projected.position;
            let mark = match source.mark {
                SourceMark::WallFace { toward, .. } => {
                    let normal = if let Some(rotation) = orientation {
                        let (x, y, z) = toward.delta();
                        let delta = tor_world::rotate_vector(
                            rotation,
                            [i64::from(x), i64::from(y), i64::from(z)],
                        );
                        let mut target = position.ok_or(Unavailable::Projection)?;
                        for i in 0..3 {
                            target[i] =
                                target[i].checked_add(delta[i]).ok_or(Unavailable::Budget)?;
                        }
                        position = Some(target);
                        Some([(-delta[0]) as i8, (-delta[1]) as i8, (-delta[2]) as i8])
                    } else {
                        None
                    };
                    MappingMark::WallFace { normal }
                }
                SourceMark::Passage => MappingMark::Passage,
                SourceMark::Doorway => MappingMark::Doorway,
                SourceMark::Stairs => MappingMark::Stairs,
                SourceMark::SecretDoor => MappingMark::SecretDoor,
                SourceMark::TrapPresence => MappingMark::TrapPresence,
            };
            ordered.push((
                Row {
                    component: projected.component,
                    position,
                    mark: Mark::Mapping(mark),
                },
                source,
            ));
        }
        sorting(ordered.len(), &mut budget)?;
        ordered.sort_unstable();
        let mut rows = reserve::<Row>(ordered.len(), &mut budget)?;
        let mut source_rows = reserve::<SourceRow>(ordered.len(), &mut budget)?;
        for (row, source) in ordered {
            rows.push(row);
            source_rows.push(source);
        }
        let mut components = reserve::<Component>(projection.components.len(), &mut budget)?;
        components.extend(projection.components.into_iter().map(|c| Component {
            alignment: c.alignment,
        }));
        pay(
            &mut budget.work,
            rows.len().checked_mul(128).ok_or(Unavailable::Budget)?,
        )?;
        pay(
            &mut budget.allocation_bytes,
            std::mem::size_of::<chart::Chart>() + 2 * std::mem::size_of::<usize>(),
        )?;
        let candidate = Candidate::from_complete(
            Kind::Mapping,
            components,
            rows,
            chart::Budget {
                components: scope.members.len(),
                rows: source_rows.len(),
                json_bytes: budget.delivery_bytes,
            },
        )
        .map_err(|_| Unavailable::Projection)?;
        let chart = candidate.chart().ok_or(Unavailable::Budget)?;
        let delivery = chart
            .delivery_summary(budget.pages)
            .map_err(|_| Unavailable::Budget)?;
        if delivery.encoded_bytes_bound > budget.delivery_bytes {
            return Err(Unavailable::Budget);
        }
        pay(&mut budget.allocation_bytes, scope.id.len())?;
        Ok(PreparedMapping {
            candidate,
            scope: Some(scope.id.clone()),
            source_rows,
        })
    };
    build().unwrap_or_else(PreparedMapping::unavailable)
}
impl Game {
    /// Supplemental immutable geometry only; never recursive floor families or
    /// transport-stair destinations. These pins do not expand chart membership.
    pub(crate) fn mapping_required_regions(
        &self,
        actor: ActorId,
    ) -> Result<std::collections::BTreeSet<tor_world::RegionId>, GameError> {
        let owner = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let Some(scope) = self
            .spatial_registry
            .scopes()
            .find(|scope| scope.members.binary_search(&owner.location.region).is_ok())
        else {
            return Ok(Default::default());
        };
        let mut needed: std::collections::BTreeSet<_> = scope.members.iter().copied().collect();
        needed.extend(
            scope
                .rooms
                .iter()
                .flat_map(|room| room.runs.iter().map(|run| run.region)),
        );
        for region in &scope.members {
            needed.extend(
                self.world
                    .exits(*region)
                    .filter(|link| !self.world.is_stair(link.from, link.direction))
                    .map(|link| link.to.region),
            );
        }
        Ok(needed)
    }
    pub(crate) fn mapping_supported(&self, actor: ActorId) -> bool {
        self.actors.get(&actor).is_some_and(|owner| {
            self.spatial_registry.scopes().any(|scope| {
                scope.members.binary_search(&owner.location.region).is_ok()
                    && matches!(scope.mapping, mapping::MappingCertification::Certified(_))
            })
        })
    }
    pub(crate) fn prepare_mapping_chart(
        &self,
        actor: ActorId,
        mut budget: CensusBudget,
    ) -> Result<PreparedMapping, GameError> {
        let owner = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        for scope in self.spatial_registry.scopes() {
            if pay(&mut budget.work, 1 + usize::BITS as usize).is_err() {
                return Ok(PreparedMapping::unavailable(Unavailable::Budget));
            }
            if scope.members.binary_search(&owner.location.region).is_ok() {
                return Ok(prepare_scope_mapping(&self.world, scope, budget));
            }
        }
        Ok(PreparedMapping::unavailable(Unavailable::Projection))
    }
}
#[cfg(test)]
#[path = "mapping_chart_tests.rs"]
pub(crate) mod tests;
