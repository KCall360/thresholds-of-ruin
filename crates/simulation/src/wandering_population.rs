//! Active-floor population, separate from once-only entry stock. Queries never
//! draw randomness; clocks and births are committed with the enclosing action.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WanderingPolicy {
    /// Actor stage shared by the enabled profiles; its depth bands and growth
    /// remain the sole source of monster selection and normal kill rewards.
    pub stage_id: String,
    #[serde(deserialize_with = "read_exclusions")]
    pub excluded_wandering_content: BTreeSet<String>,
    pub initial_delay: [u64; 2],
    pub retry_ticks: u64,
    pub chance: CarryChance,
    pub living_cap: u16,
}
impl<'de> Deserialize<'de> for WanderingPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            stage_id: String,
            #[serde(deserialize_with = "read_exclusions")]
            excluded_wandering_content: BTreeSet<String>,
            initial_delay: [u64; 2],
            retry_ticks: u64,
            chance: CarryChance,
            living_cap: u16,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let policy = Self {
            stage_id: f.stage_id,
            excluded_wandering_content: f.excluded_wandering_content,
            initial_delay: f.initial_delay,
            retry_ticks: f.retry_ticks,
            chance: f.chance,
            living_cap: f.living_cap,
        };
        if !policy.valid() {
            return Err(serde::de::Error::custom("invalid wandering policy"));
        }
        Ok(policy)
    }
}
impl WanderingPolicy {
    pub fn valid(&self) -> bool {
        label(&self.stage_id)
            && self.excluded_wandering_content.len() <= MAX_CHOICES
            && self.excluded_wandering_content.iter().all(|id| label(id))
            && self.initial_delay[0] > 0
            && self.initial_delay[0] <= self.initial_delay[1]
            && self.initial_delay[1] <= u64::from(u32::MAX)
            && self.retry_ticks > 0
            && self.retry_ticks <= u64::from(u32::MAX)
            && self.chance.valid()
            && self.living_cap > 0
            && self.living_cap <= MAX_ATTEMPTS
    }
    fn delay(&self, rng: &mut u64) -> u64 {
        self.initial_delay[0]
            + index(
                rng,
                (self.initial_delay[1] - self.initial_delay[0] + 1) as usize,
            ) as u64
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct FloorClock {
    remaining: u64,
    settled_at: u64,
    active: bool,
}
impl<'de> Deserialize<'de> for FloorClock {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            remaining: u64,
            settled_at: u64,
            active: bool,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            remaining: f.remaining,
            settled_at: f.settled_at,
            active: f.active,
        })
    }
}
impl FloorClock {
    fn settle(&mut self, tick: u64) {
        if self.active {
            self.remaining = self
                .remaining
                .saturating_sub(tick.saturating_sub(self.settled_at));
        }
        self.settled_at = tick;
    }
    fn deadline(&self) -> Option<u64> {
        self.active
            .then(|| self.settled_at.checked_add(self.remaining))
            .flatten()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BirthOrigin {
    Wandering,
    Created,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct LivingBirth {
    scope: String,
    origin: BirthOrigin,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WanderingPopulation {
    policy: WanderingPolicy,
    #[serde(deserialize_with = "read_clocks")]
    clocks: Shared<BTreeMap<String, FloorClock>>,
    #[serde(deserialize_with = "read_births")]
    births: Shared<BTreeMap<ActorId, LivingBirth>>,
}
impl<'de> Deserialize<'de> for LivingBirth {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            scope: String,
            origin: BirthOrigin,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            scope: f.scope,
            origin: f.origin,
        })
    }
}
impl<'de> Deserialize<'de> for WanderingPopulation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            policy: WanderingPolicy,
            #[serde(deserialize_with = "read_clocks")]
            clocks: Shared<BTreeMap<String, FloorClock>>,
            #[serde(deserialize_with = "read_births")]
            births: Shared<BTreeMap<ActorId, LivingBirth>>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            policy: f.policy,
            clocks: f.clocks,
            births: f.births,
        })
    }
}
fn read_exclusions<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeSet<String>, D::Error> {
    let values = crate::bounded::Bounded::<String, MAX_CHOICES>::deserialize(d)?.0;
    let count = values.len();
    let set: BTreeSet<_> = values.into_iter().collect();
    if set.len() != count {
        return Err(serde::de::Error::custom("duplicate wandering exclusion"));
    }
    Ok(set)
}
fn read_clocks<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<BTreeMap<String, FloorClock>>, D::Error> {
    bounded_map::<_, _, _, MAX_SCOPES>(d).map(Shared::new)
}
fn read_births<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<BTreeMap<ActorId, LivingBirth>>, D::Error> {
    bounded_map::<_, _, _, MAX_RETAINED_DECISIONS>(d).map(Shared::new)
}

impl Game {
    pub(crate) fn same_population_scope(&self, a: Location, b: Location) -> bool {
        match self.activation_scope_at(a.region) {
            Some(scope) => scope.members.contains(&b.region),
            None => a.region == b.region,
        }
    }

    pub fn configure_wandering_population(
        &mut self,
        policy: WanderingPolicy,
    ) -> Result<(), GameError> {
        let state = self.floor_activation.as_ref().ok_or_else(invalid)?;
        if self.tick != 0
            || state.wandering.is_some()
            || !state.floors.is_empty()
            || !policy.valid()
            || state.plan.profiles.values().any(|profile| {
                !profile.stages.iter().any(|stage| {
                    stage.id == policy.stage_id
                        && matches!(stage.population, PopulationContent::Actors { .. })
                })
            })
        {
            return Err(invalid());
        }
        self.floor_activation.as_mut().unwrap().wandering = Some(WanderingPopulation {
            policy,
            clocks: Shared::default(),
            births: Shared::default(),
        });
        Ok(())
    }

    pub fn wandering_policy(&self) -> Option<&WanderingPolicy> {
        self.floor_activation
            .as_ref()?
            .wandering
            .as_ref()
            .map(|state| &state.policy)
    }

    /// Called after activation and after residency changes. The caller settles
    /// clocks BEFORE changing residency, so leaving a floor never loses elapsed
    /// active time and entering one never charges inactive global time.
    pub(crate) fn reconcile_wandering_activity(&mut self) -> Result<(), GameError> {
        let Some(state) = self.floor_activation.as_ref() else {
            return Ok(());
        };
        let Some(wandering) = state.wandering.as_ref() else {
            return Ok(());
        };
        let policy = wandering.policy.clone();
        let activity: Vec<_> = state
            .floors
            .keys()
            .map(|key| {
                let scope = &state.scopes[key];
                let active = scope.members.iter().any(|region| {
                    self.world.region(*region).is_some() && !self.region_frozen(*region)
                });
                (key.clone(), active)
            })
            .collect();
        let mut rng = self.rules_rng;
        let wandering = self
            .floor_activation
            .as_mut()
            .unwrap()
            .wandering
            .as_mut()
            .unwrap();
        for (key, active) in activity {
            let clock = wandering.clocks.entry(key).or_insert_with(|| FloorClock {
                remaining: policy.delay(&mut rng),
                settled_at: self.tick,
                active,
            });
            // Entry activation can run between a physics tick increment and
            // its final timer pass. Preserve that old-activity fraction too;
            // transition callers already settled it, making this idempotent.
            clock.settle(self.tick);
            clock.active = active;
            if active && clock.deadline().is_none() {
                return Err(GameError::Schedule(crate::ScheduleError::TimeExhausted));
            }
        }
        self.rules_rng = rng;
        Ok(())
    }

    pub(crate) fn next_wandering_event_tick(&self) -> Option<u64> {
        self.floor_activation
            .as_ref()?
            .wandering
            .as_ref()?
            .clocks
            .values()
            .filter_map(FloorClock::deadline)
            .min()
    }

    pub(crate) fn settle_wandering_population(&mut self) -> Result<(), GameError> {
        let Some(state) = self.floor_activation.as_mut() else {
            return Ok(());
        };
        let Some(wandering) = state.wandering.as_mut() else {
            return Ok(());
        };
        // Detached actors cannot die while frozen. Retire loaded corpses
        // before a transition can hide their health in a region record; the
        // retained detached origins therefore still represent living capacity.
        wandering
            .births
            .retain(|id, _| self.actors.get(id).is_none_or(|actor| actor.alive()));
        let policy = wandering.policy.clone();
        let due: Vec<_> = wandering
            .clocks
            .iter_mut()
            .filter_map(|(key, clock)| {
                clock.settle(self.tick);
                (clock.active && clock.remaining == 0).then(|| key.clone())
            })
            .collect();
        for key in due {
            let mut rng = self.rules_rng;
            let succeeds = policy.chance.succeeds(&mut rng);
            self.rules_rng = rng;
            if succeeds {
                self.spawn_living_population(&key, BirthOrigin::Wandering, None)?;
            }
            let mut rng = self.rules_rng;
            let remaining = if succeeds {
                policy.delay(&mut rng)
            } else {
                policy.retry_ticks
            };
            self.tick
                .checked_add(remaining)
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
            self.rules_rng = rng;
            self.floor_activation
                .as_mut()
                .unwrap()
                .wandering
                .as_mut()
                .unwrap()
                .clocks
                .get_mut(&key)
                .unwrap()
                .remaining = remaining;
        }
        Ok(())
    }

    /// Called only when the paid readable commits. A blocked neighborhood is a
    /// successful, consumed read with no birth, never a free validation refusal.
    /// This origin bypasses AND does not consume wandering capacity.
    pub(crate) fn create_population_monster(
        &mut self,
        reader: ActorId,
    ) -> Result<Option<ActorId>, GameError> {
        let actor = self.actors.get(&reader).ok_or(GameError::UnknownActor)?;
        let Some(state) = self.floor_activation.as_ref() else {
            return Ok(None);
        };
        let Some(key) = state.members.get(&actor.location.region).cloned() else {
            return Ok(None);
        };
        self.spawn_living_population(&key, BirthOrigin::Created, Some(reader))
    }

    fn spawn_living_population(
        &mut self,
        key: &str,
        origin: BirthOrigin,
        reader: Option<ActorId>,
    ) -> Result<Option<ActorId>, GameError> {
        let state = self.floor_activation.as_ref().ok_or_else(invalid)?;
        let Some(wandering) = state.wandering.as_ref() else {
            return Ok(None);
        };
        let scope = state.scopes.get(key).ok_or_else(invalid)?.clone();
        let plan = state.plan.clone();
        let profile = &plan.profiles[&scope.profile];
        let stage = profile
            .stages
            .iter()
            .find(|stage| stage.id == wandering.policy.stage_id)
            .ok_or_else(invalid)?;
        let PopulationContent::Actors { bands, .. } = &stage.population else {
            return Err(invalid());
        };
        let Some(band) = eligible_band(bands, scope.depth) else {
            return Ok(None);
        };
        if origin == BirthOrigin::Wandering {
            let living = wandering
                .births
                .iter()
                .filter(|(id, birth)| {
                    birth.origin == BirthOrigin::Wandering
                        && self
                            .known_actor_region(**id)
                            .is_some_and(|region| scope.members.contains(&region))
                        && self.actors.get(id).is_none_or(|actor| actor.alive())
                })
                .count();
            if living >= usize::from(wandering.policy.living_cap) {
                return Ok(None);
            }
        }
        if wandering.births.len() >= MAX_RETAINED_DECISIONS {
            return Err(GameError::IdentityExhausted);
        }
        let choices: Vec<_> = band
            .choices
            .iter()
            .filter(|choice| {
                origin != BirthOrigin::Wandering
                    || !wandering
                        .policy
                        .excluded_wandering_content
                        .contains(&choice.content)
            })
            .cloned()
            .collect();
        if choices.is_empty() {
            return Ok(None);
        }
        let mut rng = self.rules_rng;
        let choice = weighted_choice(&mut rng, &choices, |choice| choice.weight);
        let body = choice.actor.body.clone().unwrap_or_default();
        let protagonist = plan.protagonist;
        let Some(pc) = self
            .actors
            .get(&protagonist)
            .map(|actor| actor.location)
            .or_else(|| reader.and_then(|id| self.actors.get(&id).map(|actor| actor.location)))
        else {
            return Ok(None);
        };
        let seen: BTreeSet<_> = if origin == BirthOrigin::Wandering {
            self.scene(protagonist)?
                .into_iter()
                .map(|cell| cell.location)
                .collect()
        } else {
            BTreeSet::new()
        };
        let room = scope.rooms.iter().find(|room| room.cells.contains(&pc));
        let mut cells = if let Some(reader) = reader {
            let at = self.actors[&reader].location;
            tor_world::Direction::HORIZONTAL
                .into_iter()
                .filter_map(|direction| match self.world.ray_step(at, direction) {
                    tor_world::RayStep::Clear { to, .. } => Some(to),
                    _ => None,
                })
                .collect::<Vec<_>>()
        } else {
            scope.cells.clone()
        };
        cells.sort();
        cells.dedup();
        for i in (1..cells.len()).rev() {
            let j = index(&mut rng, i + 1);
            cells.swap(i, j);
        }
        let static_eligible = |at: Location| {
            scope.members.contains(&at.region)
                && !scope.protected.contains(&at)
                && !seen.contains(&at)
                && (origin != BirthOrigin::Wandering
                    || room.is_none_or(|room| !room.cells.contains(&at)))
        };
        let loaded_eligible = |at: Location| {
            static_eligible(at)
                && self.world.walkable(at)
                && !self
                    .actors
                    .at(&self.world, at)
                    .keys()
                    .any(|id| self.actors[id].alive())
        };
        let mut at = None;
        for candidate in cells {
            if scope.cells.binary_search(&candidate).is_err() || !static_eligible(candidate) {
                continue;
            }
            // Keep the full authored-floor draw. Unknown geometry is a bounded
            // miss, not a reason to preload remote rooms or favor resident cells.
            // Create's cells already came only from resolved adjacent ray steps.
            if self.world.region(candidate.region).is_none() {
                break;
            }
            if loaded_eligible(candidate)
                && self.body_fits(ActorId(0), candidate, 0, &body)
                && self
                    .body_cells(candidate, 0, &body)
                    .is_some_and(|cells| cells.iter().all(|(cell, _)| loaded_eligible(*cell)))
            {
                at = Some(candidate);
                break;
            }
        }
        self.rules_rng = rng;
        let Some(at) = at else {
            return Ok(None);
        };
        let mut rng = self.rules_rng;
        let seed = crate::dice::random_word(&mut rng);
        let build = choice
            .actor
            .instantiate_build(
                self.content.clone(),
                seed,
                choice.depth_growth,
                scope.depth,
                &choice.advancement,
            )
            .map_err(GameError::from)?;
        // Publish the build-seed draw before construction: configuring a
        // creature/AI may commit awareness draws of its own. Never replace
        // that advanced stream with this earlier local snapshot afterwards.
        self.rules_rng = rng;
        let (actor, _) = self.spawn_prepared_actor_in_candidate(
            &choice.actor,
            at,
            NonZeroU64::new(choice.turn_ticks).ok_or_else(invalid)?,
            build,
            None,
        )?;
        self.floor_activation
            .as_mut()
            .unwrap()
            .wandering
            .as_mut()
            .unwrap()
            .births
            .insert(
                actor,
                LivingBirth {
                    scope: key.to_owned(),
                    origin,
                },
            );
        if origin == BirthOrigin::Wandering {
            // One finite spawn-time location lead, never a target identity or
            // a pointer that follows the protagonist's later movement.
            self.install_location_alert(actor, pc)?;
        }
        let scene = self.scene(actor)?;
        self.refresh_navigation_scene(actor, &scene);
        Ok(Some(actor))
    }

    pub(super) fn wandering_state_valid(&self) -> bool {
        let Some(state) = self.floor_activation.as_ref() else {
            return true;
        };
        let Some(wandering) = state.wandering.as_ref() else {
            return true;
        };
        let initial: BTreeSet<_> = state
            .floors
            .values()
            .flat_map(|floor| floor.decisions.iter())
            .flat_map(|decision| decision.result.attempts())
            .filter_map(|attempt| match attempt {
                PlacementDecision::Placed {
                    entity: PopulationEntity::Actor { id, .. },
                    ..
                } => Some(*id),
                _ => None,
            })
            .collect();
        wandering.policy.valid()
            && wandering.clocks.len() == state.floors.len()
            && wandering.births.len() <= MAX_RETAINED_DECISIONS
            && state.plan.profiles.values().all(|profile| {
                profile.stages.iter().any(|stage| {
                    stage.id == wandering.policy.stage_id
                        && matches!(stage.population, PopulationContent::Actors { .. })
                })
            })
            && wandering.clocks.iter().all(|(key, clock)| {
                state.floors.contains_key(key)
                    && clock.settled_at <= self.tick
                    && clock.remaining > 0
                    && clock.remaining
                        <= wandering.policy.initial_delay[1].max(wandering.policy.retry_ticks)
                    && (!clock.active
                        || clock
                            .deadline()
                            .is_some_and(|deadline| deadline >= self.tick))
                    && state.scopes.get(key).is_some_and(|scope| {
                        clock.active
                            == scope.members.iter().any(|region| {
                                self.world.region(*region).is_some() && !self.region_frozen(*region)
                            })
                    })
            })
            && wandering.births.iter().all(|(id, birth)| {
                id.0 > 0
                    && id.0 < self.next_actor_id
                    && state.floors.contains_key(&birth.scope)
                    && !initial.contains(id)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inactive_floor_preserves_remaining_and_restarts_without_catchup() {
        let mut clock = FloorClock {
            remaining: 7000,
            settled_at: 10,
            active: true,
        };
        clock.settle(1010);
        assert_eq!(clock.remaining, 6000);
        clock.active = false;
        clock.settle(1_000_010);
        assert_eq!(clock.remaining, 6000);
        assert_eq!(clock.deadline(), None);
        clock.active = true;
        assert_eq!(clock.deadline(), Some(1_006_010));
    }
    #[test]
    fn clock_roundtrip_retains_fractional_progress_and_queries_do_not_mutate() {
        let clock = FloorClock {
            remaining: 237,
            settled_at: 500,
            active: false,
        };
        let encoded = serde_json::to_value(&clock).unwrap();
        let restored: FloorClock = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(restored, clock);
        assert_eq!(restored.deadline(), None);
        assert_eq!(serde_json::to_value(restored).unwrap(), encoded);
        let mut missing = encoded.clone();
        missing.as_object_mut().unwrap().remove("active");
        assert!(serde_json::from_value::<FloorClock>(missing).is_err());
        let mut extra = encoded;
        extra["unknown"] = serde_json::json!(0);
        assert!(serde_json::from_value::<FloorClock>(extra).is_err());
    }
    #[test]
    fn policy_rejects_zero_unbounded_and_inverted_intervals() {
        let policy = WanderingPolicy {
            stage_id: "rogue.foes".into(),
            excluded_wandering_content: BTreeSet::new(),
            initial_delay: [6700, 7300],
            retry_ticks: 400,
            chance: CarryChance {
                numerator: 1,
                denominator: 6,
            },
            living_cap: 8,
        };
        assert!(policy.valid());
        let mut bad = policy.clone();
        bad.initial_delay = [7300, 6700];
        assert!(!bad.valid());
        let mut bad = policy.clone();
        bad.retry_ticks = 0;
        assert!(!bad.valid());
        let mut bad = policy.clone();
        bad.chance.denominator = 0;
        assert!(!bad.valid());
        let mut bad = policy;
        bad.living_cap = 0;
        assert!(!bad.valid());
    }
    fn at(x: i32, y: i32) -> Location {
        Location {
            region: RegionId(1),
            position: tor_world::Position { x, y, z: 0 },
        }
    }
    fn configured() -> Game {
        configured_regions(false)
    }
    fn configured_regions(multiple: bool) -> Game {
        let content = crate::test_creatures::content();
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        world
            .add_region(tor_world::Region {
                id: RegionId(1),
                name: "population".into(),
                bounds: tor_world::Extent::new(64, 3, 3).unwrap(),
            })
            .unwrap();
        if multiple {
            for region in [2, 3] {
                world
                    .add_region(tor_world::Region {
                        id: RegionId(region),
                        name: format!("room {region}"),
                        bounds: tor_world::Extent::new(64, 3, 3).unwrap(),
                    })
                    .unwrap();
            }
        }
        let mut game = Game::new(world, 42, content);
        game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
            .unwrap();
        game.spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let choice = PreparedActorChoice {
            carried_loot: None,
            depth_growth: DepthGrowth::Fixed {},
            advancement: vec![],
            content: "ordinary".into(),
            weight: 1,
            turn_ticks: 100,
            actor: PreparedActor {
                build: None,
                nutrition: crate::nutrition::NutritionRequirement::Exempt,
                identity: None,
                anatomy: None,
                body: None,
                ai: None,
                known_identities: vec![],
                velocity: None,
                asset: None,
            },
        };
        game.configure_population(PopulationPlan {
            protagonist: ActorId(1),
            carried_item_pools: BTreeMap::new(),
            profiles: BTreeMap::from([(
                "baseline".into(),
                PopulationProfile {
                    version: 1,
                    stages: vec![PopulationStage {
                        id: "foes".into(),
                        version: 1,
                        count: [0, 0],
                        population: PopulationContent::Actors {
                            bands: vec![DepthBand {
                                depth: DepthInterval::Open { minimum: 1 },
                                choices: vec![choice],
                            }],
                            avoid_protagonist_sight: false,
                        },
                    }],
                },
            )]),
        })
        .unwrap();
        game.configure_wandering_population(WanderingPolicy {
            stage_id: "foes".into(),
            excluded_wandering_content: BTreeSet::new(),
            initial_delay: [6700, 7300],
            retry_ticks: 400,
            chance: CarryChance {
                numerator: 1,
                denominator: 1,
            },
            living_cap: 8,
        })
        .unwrap();
        let mut cells = (0..64)
            .flat_map(|x| (0..3).map(move |y| at(x, y)))
            .collect::<Vec<_>>();
        if multiple {
            cells.extend((0..64).flat_map(|x| {
                (0..3).map(move |y| Location {
                    region: RegionId(2),
                    ..at(x, y)
                })
            }));
        }
        cells.sort();
        let scope = PreparedScope {
            scope: "floor/1".into(),
            depth: 1,
            profile: "baseline".into(),
            members: if multiple {
                BTreeSet::from([RegionId(1), RegionId(2)])
            } else {
                BTreeSet::from([RegionId(1)])
            },
            cells,
            protected: BTreeSet::new(),
            rooms: vec![],
        };
        // Production scopes encode their depth in the final path component.
        // Validate this positive fixture before any behavioral mutation.
        assert!(scope.valid());
        game.register_activation_scope(scope).unwrap();
        game.activate_initial_floor().unwrap();
        assert!(game.activation_state_valid());
        game
    }
    #[test]
    fn created_birth_is_adjacent_without_stock_and_has_distinct_origin() {
        let mut game = configured();
        let before_items = game.next_item_id;
        let id = game.create_population_monster(ActorId(1)).unwrap().unwrap();
        let location = game.actors[&id].location;
        assert!((location.position.x - 1).abs() <= 1 && (location.position.y - 1).abs() <= 1);
        assert_ne!(location, at(1, 1));
        assert_eq!(game.next_item_id, before_items);
        let population = game
            .floor_activation
            .as_ref()
            .unwrap()
            .wandering
            .as_ref()
            .unwrap();
        assert_eq!(population.births[&id].origin, BirthOrigin::Created);
        assert!(game.activation_state_valid());
    }
    #[test]
    fn blocked_creation_allocates_nothing_and_winning_trial_rearms_without_debt() {
        let mut game = configured();
        for x in 0..=2 {
            for y in 0..=2 {
                if (x, y) != (1, 1) {
                    game.spawn_actor(at(x, y), NonZeroU64::new(100).unwrap())
                        .unwrap();
                }
            }
        }
        let next = game.next_actor_id;
        assert_eq!(game.create_population_monster(ActorId(1)).unwrap(), None);
        assert_eq!(game.next_actor_id, next);
        // Excluding the only recipe is a bounded failed trial, not a debt.
        game.floor_activation
            .as_mut()
            .unwrap()
            .wandering
            .as_mut()
            .unwrap()
            .policy
            .excluded_wandering_content
            .insert("ordinary".into());
        game.tick = game.next_wandering_event_tick().unwrap();
        game.settle_wandering_population().unwrap();
        assert_eq!(game.next_actor_id, next);
        let remaining = game.next_wandering_event_tick().unwrap() - game.tick;
        assert!((6700..=7300).contains(&remaining));
        let rng = game.rules_rng;
        game.settle_wandering_population().unwrap();
        assert_eq!(game.rules_rng, rng);
        assert!(game.activation_state_valid());
    }
    #[test]
    fn required_population_state_survives_checkpoint_and_rejects_missing_clock_field() {
        let game = configured();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut context = crate::checkpoint::RestoreContext::new(&shared, game.content.clone());
        assert_eq!(context.restore(snapshot).unwrap(), game);
        let encoded = serde_json::to_value(game.floor_activation.as_ref().unwrap()).unwrap();
        let restored: ActivationState = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(&restored, game.floor_activation.as_ref().unwrap());
        let mut malformed = encoded;
        malformed["wandering"]["clocks"]["floor/1"]
            .as_object_mut()
            .unwrap()
            .remove("remaining");
        assert!(serde_json::from_value::<ActivationState>(malformed).is_err());
    }
    #[test]
    fn eight_live_wanderers_exclude_initial_and_created_and_death_releases_a_slot() {
        let mut game = configured();
        // This ordinary preexisting actor has no wandering origin.
        game.spawn_actor(at(60, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        let sight: BTreeSet<_> = game
            .scene(ActorId(1))
            .unwrap()
            .into_iter()
            .map(|cell| cell.location)
            .collect();
        let mut wanderers = Vec::new();
        for _ in 0..8 {
            let id = game
                .spawn_living_population("floor/1", BirthOrigin::Wandering, None)
                .unwrap()
                .unwrap();
            assert!(!sight.contains(&game.actors[&id].location));
            wanderers.push(id);
        }
        let rng = game.rules_rng;
        assert_eq!(
            game.spawn_living_population("floor/1", BirthOrigin::Wandering, None)
                .unwrap(),
            None
        );
        assert_eq!(game.rules_rng, rng);
        let created = game.create_population_monster(ActorId(1)).unwrap().unwrap();
        assert!(game.alive(created));
        assert_eq!(
            game.spawn_living_population("floor/1", BirthOrigin::Wandering, None)
                .unwrap(),
            None
        );
        crate::test_creatures::configure(
            &mut game,
            wanderers[0],
            "hostile",
            crate::test_creatures::species(),
        );
        crate::test_creatures::injure(&mut game, wanderers[0], 1000).unwrap();
        assert!(!game.alive(wanderers[0]));
        assert!(game
            .spawn_living_population("floor/1", BirthOrigin::Wandering, None)
            .unwrap()
            .is_some());
        assert!(game.activation_state_valid());
    }
    #[test]
    fn wandering_exclusions_and_current_room_are_not_create_exclusions() {
        let mut game = configured();
        // A room can extend beyond sight. It must still be excluded in full.
        let state = game.floor_activation.as_mut().unwrap();
        let mut scope = (**state.scopes.get("floor/1").unwrap()).clone();
        scope.rooms = vec![PreparedRoomGroup {
            id: 1,
            cells: scope.cells.clone(),
        }];
        state.scopes.insert("floor/1".into(), Shared::new(scope));
        assert_eq!(
            game.spawn_living_population("floor/1", BirthOrigin::Wandering, None)
                .unwrap(),
            None
        );
        game.floor_activation
            .as_mut()
            .unwrap()
            .wandering
            .as_mut()
            .unwrap()
            .policy
            .excluded_wandering_content
            .insert("ordinary".into());
        assert!(game
            .create_population_monster(ActorId(1))
            .unwrap()
            .is_some());
    }
    #[test]
    fn tall_wanderer_uses_ground_anchor_and_keeps_only_finite_spawn_location_lead() {
        let mut game = configured();
        let state = game.floor_activation.as_mut().unwrap();
        let mut plan = (*state.plan).clone();
        let PopulationContent::Actors { bands, .. } =
            &mut plan.profiles.get_mut("baseline").unwrap().stages[0].population
        else {
            unreachable!()
        };
        let definition = &mut bands[0].choices[0].actor;
        definition.build = Some(crate::creatures::record::BuildRecord::capture(
            &crate::test_creatures::build(crate::test_creatures::species()),
        ));
        definition.identity = Some(crate::CreatureIdentity {
            name: "wanderer".into(),
            faction: "hostile".into(),
        });
        definition.body = Some(crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        });
        state.plan = Shared::new(plan);
        assert!(game.activation_state_valid());
        let id = game
            .spawn_living_population("floor/1", BirthOrigin::Wandering, None)
            .unwrap()
            .unwrap();
        assert_eq!(game.actors[&id].location.position.z, 0);
        let alert = game.actors[&id].awareness.alert.unwrap();
        assert_eq!(alert.location, at(1, 1));
        assert_eq!(alert.expires, 1000);
        game.teleport(ActorId(1), at(3, 1)).unwrap();
        assert_eq!(game.actors[&id].awareness.alert.unwrap().location, at(1, 1));
        assert_eq!(game.actors[&id].awareness.alert_at(1000), None);
        let created = game.create_population_monster(ActorId(1)).unwrap().unwrap();
        assert!(game.actors[&created].awareness.alert.is_none());
    }
    fn partial_transition(
        game: &mut Game,
        active: &[u64],
        loaded: &[u64],
        records: &mut crate::MemoryRecords,
    ) {
        let (_, _, _) = game
            .transition_regions_counted_deferred(
                &crate::RegionTransition {
                    active: active.iter().copied().map(RegionId).collect(),
                    loaded: loaded.iter().copied().map(RegionId).collect(),
                },
                records,
            )
            .unwrap();
    }

    #[test]
    fn revisited_partial_floor_allows_adjacent_creation_and_resident_wandering_without_preload() {
        let mut game = configured_regions(true);
        let mut records = crate::MemoryRecords::default();
        partial_transition(&mut game, &[1], &[1, 3], &mut records);
        assert!(game.world.region(RegionId(2)).is_none());
        assert!(game
            .floor_activation
            .as_ref()
            .unwrap()
            .floors
            .contains_key("floor/1"));
        let created = game.create_population_monster(ActorId(1)).unwrap().unwrap();
        assert_eq!(game.actors[&created].location.region, RegionId(1));
        let mut misses = 0;
        let mut births = 0;
        for _ in 0..64 {
            match game
                .spawn_living_population("floor/1", BirthOrigin::Wandering, None)
                .unwrap()
            {
                Some(id) => {
                    births += 1;
                    assert_eq!(game.actors[&id].location.region, RegionId(1));
                }
                None => misses += 1,
            }
        }
        assert!(
            births > 0,
            "partial residency must not categorically suppress wanderers"
        );
        assert!(
            misses > 0,
            "unavailable candidates/cap remain bounded misses"
        );
        assert!(
            game.world.region(RegionId(2)).is_none(),
            "spawning never preloads remote geometry"
        );
        assert!(game.activation_state_valid());
    }

    #[test]
    fn detached_live_wanderers_keep_capacity_and_a_dead_detached_body_does_not() {
        let mut game = configured_regions(true);
        let mut ids = Vec::new();
        for _ in 0..8 {
            ids.push(
                game.spawn_living_population("floor/1", BirthOrigin::Wandering, None)
                    .unwrap()
                    .unwrap(),
            );
        }
        for (offset, id) in ids.iter().enumerate() {
            game.teleport(
                *id,
                Location {
                    region: RegionId(2),
                    ..at(20 + offset as i32, 1)
                },
            )
            .unwrap();
        }
        let mut records = crate::MemoryRecords::default();
        partial_transition(&mut game, &[1], &[1, 3], &mut records);
        assert!(ids.iter().all(|id| !game.actors.contains_key(id)));
        assert_eq!(
            game.spawn_living_population("floor/1", BirthOrigin::Wandering, None)
                .unwrap(),
            None
        );
        assert!(game
            .create_population_monster(ActorId(1))
            .unwrap()
            .is_some());
        partial_transition(&mut game, &[1, 2], &[1, 2, 3], &mut records);
        crate::test_creatures::configure(
            &mut game,
            ids[0],
            "hostile",
            crate::test_creatures::species(),
        );
        crate::test_creatures::injure(&mut game, ids[0], 1000).unwrap();
        assert!(!game.alive(ids[0]));
        partial_transition(&mut game, &[1], &[1, 3], &mut records);
        assert!(!game
            .floor_activation
            .as_ref()
            .unwrap()
            .wandering
            .as_ref()
            .unwrap()
            .births
            .contains_key(&ids[0]));
        let mut born = None;
        for _ in 0..64 {
            born = game
                .spawn_living_population("floor/1", BirthOrigin::Wandering, None)
                .unwrap();
            if born.is_some() {
                break;
            }
        }
        assert!(
            born.is_some(),
            "one death releases one slot even after its corpse detaches"
        );
        assert_eq!(
            game.spawn_living_population("floor/1", BirthOrigin::Wandering, None)
                .unwrap(),
            None
        );
    }

    #[test]
    fn full_game_floor_clock_freezes_through_checkpoint_and_resumes_same_fraction() {
        let mut game = configured_regions(true);
        let mut records = crate::MemoryRecords::default();
        let initial_deadline = game.next_wandering_event_tick().unwrap();
        game.act(ActorId(1), crate::Action::Wait).unwrap();
        assert!(game.tick > 0 && game.tick < initial_deadline);
        partial_transition(&mut game, &[1], &[1, 3], &mut records);
        game.teleport(
            ActorId(1),
            Location {
                region: RegionId(3),
                ..at(1, 1)
            },
        )
        .unwrap();
        partial_transition(&mut game, &[3], &[1, 3], &mut records);
        let frozen_at = game.tick;
        let remaining = initial_deadline - frozen_at;
        assert_eq!(game.next_wandering_event_tick(), None);
        for _ in 0..5 {
            game.act(ActorId(1), crate::Action::Wait).unwrap();
        }
        assert!(game.tick > frozen_at);
        let rng = game.rules_rng;
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut context = crate::checkpoint::RestoreContext::new(&shared, game.content.clone());
        let mut resumed = context.restore(snapshot).unwrap();
        assert_eq!(resumed, game);
        assert_eq!(resumed.rules_rng, rng);
        assert_eq!(resumed.next_wandering_event_tick(), None);
        partial_transition(&mut resumed, &[1, 3], &[1, 3], &mut records);
        assert_eq!(
            resumed.next_wandering_event_tick(),
            Some(resumed.tick + remaining)
        );
        assert!(resumed.world.region(RegionId(2)).is_none());
        assert!(resumed.activation_state_valid());
    }
}
