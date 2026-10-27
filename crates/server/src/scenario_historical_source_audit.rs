//! Retained canonical scope/source metadata for immutable historical chart roots.
//!
//! The captured World is lawful saved state, not a claim that no wizard or paid
//! terrain/topology mutation ever happened. Core must validate every captured
//! World once via World::historical_checkpoint_valid before this host audit.
//! No wall/portal/trap-event equality or second event-history authority is used.
use super::*;
use tor_world::{Shared, SharedIdentity};

fn invalid() -> Failure {
    Failure::new(
        tor_protocol::ErrorCode::InvalidArchive,
        "Historical chart source metadata differs from canonical source",
    )
}

struct Memo {
    world_address: usize,
    registry_address: usize,
    world: SharedIdentity<World>,
    registry: SharedIdentity<Registry>,
    scope: String,
}
impl Memo {
    fn key(&self) -> (usize, usize, &str) {
        (self.world_address, self.registry_address, &self.scope)
    }
}

#[derive(Clone, Copy)]
struct SourceExtent {
    region: RegionId,
    bounds: tor_world::Extent,
    chamber: bool,
}
struct CanonicalSource {
    scope: Scope,
    extents: Vec<SourceExtent>,
}

// Exact owned capacities, not expanded cell counts or a second encoded cap.
// The compiler-owned Scope moves into the cache; it is never cloned here.
fn scope_heap_bytes(scope: &Scope) -> Option<usize> {
    fn vector<T>(values: &Vec<T>) -> Option<usize> {
        values.capacity().checked_mul(std::mem::size_of::<T>())
    }
    let mut bytes = scope
        .id
        .capacity()
        .checked_add(vector(&scope.members)?)?
        .checked_add(vector(&scope.primary)?)?
        .checked_add(vector(&scope.rooms)?)?;
    for room in &scope.rooms {
        bytes = bytes.checked_add(vector(&room.runs)?)?;
    }
    if let tor_simulation::spatial_scope::mapping::MappingCertification::Certified(frames) =
        &scope.mapping
    {
        bytes = bytes
            .checked_add(vector(&frames.room_frames)?)?
            .checked_add(vector(&frames.exterior_doorways)?)?;
    }
    Some(bytes)
}

#[cfg(test)]
#[derive(Default, Debug, PartialEq, Eq)]
struct AuditCounts {
    // Only reconstruction requested by this historical hook. Package source
    // admission in new() is separate, constant work for the restore pass.
    source_reconstructions: usize,
    scope_comparisons: usize,
    extent_checks: usize,
    memo_hits: usize,
}

/// One restore pass owns one immutable selected Package/seed authority. Canonical
/// source Scope and compact required extents may be reused by scope ID; source
/// Worlds/definitions die after reconstruction. Captured Worlds are only Weak.
pub(crate) struct HistoricalSourceAudit<'a> {
    package: Option<&'a Package>,
    seed: u64,
    memo: Vec<Memo>,
    memo_bytes: usize,
    canonical: Vec<CanonicalSource>,
    canonical_bytes: usize,
    #[cfg(test)]
    counts: AuditCounts,
}
impl<'a> HistoricalSourceAudit<'a> {
    pub(crate) fn new(package: Option<&'a Package>, seed: u64) -> Result<Self, Failure> {
        if let Some(package) = package {
            package.validate_spatial_sources()?;
        }
        Ok(Self {
            package,
            seed,
            memo: Vec::new(),
            memo_bytes: 0,
            canonical: Vec::new(),
            canonical_bytes: 0,
            #[cfg(test)]
            counts: AuditCounts::default(),
        })
    }

    /// Precondition: the core historical-root validator has accepted this World
    /// and its chart references. Repeating full structural validation here would
    /// duplicate the once-per-shared-World restore/capture pass.
    pub(crate) fn check_shared(
        &mut self,
        world: &Shared<World>,
        registry: &Shared<Registry>,
        scope_id: &str,
    ) -> Result<(), Failure> {
        // Addresses index the sorted memo only. A hit MUST match both retained
        // weak identities; addresses/geometry_snapshot are never authority.
        // Keeping the Weak allocation control blocks also prevents address reuse.
        let world_address = std::ptr::from_ref(&**world) as usize;
        let registry_address = std::ptr::from_ref(&**registry) as usize;
        let key = (world_address, registry_address, scope_id);
        let position = match self.memo.binary_search_by(|entry| entry.key().cmp(&key)) {
            Ok(i)
                if self.memo[i].world.matches(world) && self.memo[i].registry.matches(registry) =>
            {
                #[cfg(test)]
                {
                    self.counts.memo_hits += 1;
                }
                return Ok(());
            }
            Ok(i) | Err(i) => i,
        };
        self.check(world, registry, scope_id)?;

        // Memoization is optional. Exhaustion never turns unverified data into
        // a hit or rejects a valid root. Precharge before either owned allocation
        // and use the unchanged archive envelope, not a new feature-specific cap.
        let Some(next) = self
            .memo_bytes
            .checked_add(std::mem::size_of::<Memo>())
            .and_then(|n| n.checked_add(scope_id.len()))
            .filter(|n| {
                n.checked_add(self.canonical_bytes)
                    .is_some_and(|total| total <= tor_simulation::checkpoint::ARCHIVE_BYTE_LIMIT)
            })
        else {
            return Ok(());
        };
        let mut scope = String::new();
        if scope.try_reserve_exact(scope_id.len()).is_err()
            || self.memo.try_reserve_exact(1).is_err()
        {
            return Ok(());
        }
        scope.push_str(scope_id);
        self.memo.insert(
            position,
            Memo {
                world_address,
                registry_address,
                world: world.weak_identity(),
                registry: registry.weak_identity(),
                scope,
            },
        );
        self.memo_bytes = next;
        Ok(())
    }

    /// Same core-validation precondition as `check_shared`; this entry bypasses
    /// identity memoization for independently supplied native roots.
    pub(crate) fn check(
        &mut self,
        world: &World,
        registry: &Registry,
        scope_id: &str,
    ) -> Result<(), Failure> {
        self.check_source(world, registry, scope_id)
            .map_err(|_| invalid())
    }

    fn check_source(
        &mut self,
        world: &World,
        registry: &Registry,
        scope_id: &str,
    ) -> Result<(), Failure> {
        let package = self.package.ok_or_else(invalid)?;
        let captured = registry
            .scopes()
            .find(|scope| scope.id == scope_id)
            .ok_or_else(invalid)?;
        let position = match self
            .canonical
            .binary_search_by(|entry| entry.scope.id.as_str().cmp(scope_id))
        {
            Ok(index) => {
                #[cfg(test)]
                {
                    self.counts.scope_comparisons += 1;
                }
                let source = &self.canonical[index];
                if captured != &source.scope {
                    return Err(invalid());
                }
                for expected in &source.extents {
                    #[cfg(test)]
                    {
                        self.counts.extent_checks += 1;
                    }
                    let captured = world.region(expected.region).ok_or_else(invalid)?;
                    if captured.bounds != expected.bounds
                        || world.is_chamber(expected.region) != expected.chamber
                    {
                        return Err(invalid());
                    }
                }
                return Ok(());
            }
            Err(index) => index,
        };
        #[cfg(test)]
        {
            self.counts.source_reconstructions += 1;
        }
        let (canonical, source) = package.canonical_scope_geometry(scope_id, self.seed)?;
        #[cfg(test)]
        {
            self.counts.scope_comparisons += 1;
        }
        if captured != &canonical {
            return Err(invalid());
        }
        let mut region_count = 0usize;
        // Every distinct captured World still proves source-required member and
        // immediate-shell extents. Core owns its once-per-World structural proof.
        // Neither cache hit nor miss compares lawful dynamic topology/solidity.
        for region in source.loaded_regions() {
            #[cfg(test)]
            {
                self.counts.extent_checks += 1;
            }
            let expected = source.region(region).ok_or_else(invalid)?;
            let captured = world.region(region).ok_or_else(invalid)?;
            if captured.bounds != expected.bounds
                || world.is_chamber(region) != source.is_chamber(region)
            {
                return Err(invalid());
            }
            region_count = region_count.checked_add(1).ok_or_else(invalid)?;
        }
        // Optional reuse shares the existing archive-sized memo allowance.
        // Charge retained Scope capacities, exact requested extent allocation,
        // and the next exact Vec slot BEFORE either cache allocation. Failure
        // leaves the already completed proof valid; future checks reconstruct
        // through the same individually metered source compiler, never skip it.
        let charge = scope_heap_bytes(&canonical)
            .and_then(|n| {
                region_count
                    .checked_mul(std::mem::size_of::<SourceExtent>())
                    .and_then(|extents| n.checked_add(extents))
            })
            .and_then(|n| n.checked_add(std::mem::size_of::<CanonicalSource>()));
        let Some(next) = charge
            .and_then(|n| self.canonical_bytes.checked_add(n))
            .filter(|n| {
                n.checked_add(self.memo_bytes)
                    .is_some_and(|total| total <= tor_simulation::checkpoint::ARCHIVE_BYTE_LIMIT)
            })
        else {
            return Ok(());
        };
        let mut extents = Vec::new();
        if extents.try_reserve_exact(region_count).is_err() {
            return Ok(());
        }
        for region in source.loaded_regions() {
            let bounds = source.region(region).ok_or_else(invalid)?.bounds;
            extents.push(SourceExtent {
                region,
                bounds,
                chamber: source.is_chamber(region),
            });
        }
        // Reserve the persistent directory last: no later fallible allocation
        // can leave uncharged spare directory capacity behind.
        if self.canonical.try_reserve_exact(1).is_err() {
            return Ok(());
        }
        self.canonical.insert(
            position,
            CanonicalSource {
                scope: canonical,
                extents,
            },
        );
        self.canonical_bytes = next;
        Ok(())
    }
}

#[cfg(test)]
mod memo_tests {
    use super::*;
    fn changed_bounds(world: &World) -> World {
        let mut value = serde_json::to_value(world).unwrap();
        value["regions"]["1"]["bounds"]["width"] = serde_json::json!(10);
        let changed: World = serde_json::from_value(value).unwrap();
        assert!(changed.historical_checkpoint_valid());
        changed
    }
    fn roots(package: &Package) -> (Shared<World>, Shared<Registry>) {
        let (scope, world) = package.canonical_scope_geometry("study", 42).unwrap();
        let mut registry = Registry::default();
        registry.register_archive_checked(scope).unwrap();
        assert!(world.historical_checkpoint_valid());
        (Shared::new(world), Shared::new(registry))
    }
    #[test]
    fn memo_requires_both_immutable_identities_and_never_pins_world_contents() {
        let package = super::super::tests::package();
        let (mut world, registry) = roots(&package);
        let mut audit = HistoricalSourceAudit::new(Some(&package), 42).unwrap();
        audit.check_shared(&world, &registry, "study").unwrap();
        audit.check_shared(&world, &registry, "study").unwrap();
        assert_eq!(audit.memo.len(), 1);
        assert!(
            !world.is_shared(),
            "memo must not retain a strong World root"
        );
        let old = world.weak_identity();
        *world = changed_bounds(&world);
        assert!(
            !old.matches(&world),
            "COW invalidates the old memo identity"
        );
        assert_eq!(
            audit
                .check_shared(&world, &registry, "study")
                .unwrap_err()
                .code,
            tor_protocol::ErrorCode::InvalidArchive
        );
        assert_eq!(audit.memo.len(), 1, "failed roots are not memoized");

        let (world, _) = roots(&package);
        let empty = Shared::new(Registry::default());
        assert!(audit.check_shared(&world, &empty, "study").is_err());
        audit.check_shared(&world, &registry, "study").unwrap();
        assert_eq!(
            audit.memo.len(),
            2,
            "equal contents do not forge pooled identity"
        );
        assert!(audit.check_shared(&world, &registry, "missing").is_err());
    }
    #[test]
    fn memo_exhaustion_precedes_growth_but_never_skips_source_verification() {
        let package = super::super::tests::package();
        let (mut world, registry) = roots(&package);
        let mut audit = HistoricalSourceAudit::new(Some(&package), 42).unwrap();
        audit.memo_bytes = tor_simulation::checkpoint::ARCHIVE_BYTE_LIMIT;
        audit.check_shared(&world, &registry, "study").unwrap();
        assert!(audit.memo.is_empty());
        assert_eq!(
            audit.memo.capacity(),
            0,
            "no speculative memo allocation after exhaustion"
        );
        *world = changed_bounds(&world);
        assert!(audit.check_shared(&world, &registry, "study").is_err());
        assert!(audit.memo.is_empty());
    }
    #[test]
    fn memo_exact_charge_boundary_admits_one_entry_and_no_second_allocation() {
        let package = super::super::tests::package();
        let (world, registry) = roots(&package);
        let mut audit = HistoricalSourceAudit::new(Some(&package), 42).unwrap();
        let charge = std::mem::size_of::<Memo>() + "study".len();
        audit.memo_bytes = tor_simulation::checkpoint::ARCHIVE_BYTE_LIMIT - charge;
        audit.check_shared(&world, &registry, "study").unwrap();
        assert_eq!(audit.memo.len(), 1);
        assert_eq!(
            audit.memo_bytes,
            tor_simulation::checkpoint::ARCHIVE_BYTE_LIMIT
        );
        let before_capacity = audit.memo.capacity();
        let (other, _) = roots(&package);
        audit.check_shared(&other, &registry, "study").unwrap();
        assert_eq!(audit.memo.len(), 1);
        assert_eq!(audit.memo.capacity(), before_capacity);
    }
    fn owned_cache_bytes(audit: &HistoricalSourceAudit<'_>) -> usize {
        audit.canonical.capacity() * std::mem::size_of::<CanonicalSource>()
            + audit
                .canonical
                .iter()
                .map(|entry| {
                    scope_heap_bytes(&entry.scope).unwrap()
                        + entry.extents.capacity() * std::mem::size_of::<SourceExtent>()
                })
                .sum::<usize>()
    }

    #[test]
    fn canonical_source_cache_many_lawful_worlds_preserve_each_registry_and_extent_proof() {
        const ROOTS: usize = 1024;
        let package = super::super::tests::package();
        let (world, registry) = roots(&package);
        let region_count = world.loaded_regions().count();
        let samples: Vec<_> = (0..ROOTS)
            .map(|n| {
                let mut next = (*world).clone();
                next.set_gravity(RegionId(1), [0, 0, (n % 17) as i32 - 8])
                    .unwrap();
                assert!(next.historical_checkpoint_valid());
                // Both allocation identities are distinct; equal registry contents
                // do not substitute for canonical equality on a new tuple.
                (Shared::new(next), Shared::new((*registry).clone()))
            })
            .collect();
        let mut audit = HistoricalSourceAudit::new(Some(&package), 42).unwrap();
        let start = std::time::Instant::now();
        for (world, registry) in &samples {
            audit.check_shared(world, registry, "study").unwrap();
        }
        let distinct_elapsed = start.elapsed();
        assert_eq!(audit.counts.source_reconstructions, 1);
        assert_eq!(audit.counts.scope_comparisons, ROOTS);
        assert_eq!(audit.counts.extent_checks, ROOTS * region_count);
        assert_eq!(audit.memo.len(), ROOTS);
        assert_eq!(audit.canonical.len(), 1);
        assert_eq!(audit.canonical_bytes, owned_cache_bytes(&audit));
        assert!(
            audit.canonical_bytes + audit.memo_bytes
                <= tor_simulation::checkpoint::ARCHIVE_BYTE_LIMIT
        );
        let start = std::time::Instant::now();
        for (world, registry) in &samples {
            audit.check_shared(world, registry, "study").unwrap();
        }
        let repeated_elapsed = start.elapsed();
        assert_eq!(audit.counts.memo_hits, ROOTS);
        assert_eq!(audit.counts.scope_comparisons, ROOTS);
        assert_eq!(audit.counts.extent_checks, ROOTS * region_count);
        assert!(samples
            .iter()
            .all(|(world, registry)| !world.is_shared() && !registry.is_shared()));
        eprintln!("historical host audit: roots={ROOTS}, reconstruct=1, scope_proofs={ROOTS}, extent_proofs={}, distinct_us={}, repeated_us={}, retained_bytes={}",
            ROOTS*region_count,distinct_elapsed.as_micros(),repeated_elapsed.as_micros(),audit.canonical_bytes+audit.memo_bytes);

        let wrong_world = Shared::new(changed_bounds(&world));
        assert!(audit
            .check_shared(&wrong_world, &registry, "study")
            .is_err());
        let mut wrong_scope = registry.scopes().next().unwrap().clone();
        wrong_scope.mapping =
            tor_simulation::spatial_scope::mapping::MappingCertification::CapacityUnavailable;
        let mut wrong_registry = Registry::default();
        wrong_registry
            .register_archive_checked(wrong_scope)
            .unwrap();
        assert!(audit
            .check_shared(&world, &Shared::new(wrong_registry), "study")
            .is_err());
        assert_eq!(
            audit.counts.source_reconstructions, 1,
            "invalid captured roots cannot force trusted source regeneration or become cache hits"
        );
        assert_eq!(
            audit.memo.len(),
            ROOTS,
            "failed roots never enter the verified tuple memo"
        );
    }

    #[test]
    fn canonical_source_cache_exact_capacity_and_one_less_fallback_remain_authority_neutral() {
        let package = super::super::tests::package();
        let (world, registry) = roots(&package);
        let mut measured = HistoricalSourceAudit::new(Some(&package), 42).unwrap();
        measured.check(&world, &registry, "study").unwrap();
        let charge = measured.canonical_bytes;
        assert_eq!(charge, owned_cache_bytes(&measured));
        assert!(charge > 0);
        let limit = tor_simulation::checkpoint::ARCHIVE_BYTE_LIMIT;
        let mut exact = HistoricalSourceAudit::new(Some(&package), 42).unwrap();
        exact.memo_bytes = limit - charge;
        exact.check(&world, &registry, "study").unwrap();
        assert_eq!(exact.canonical.len(), 1);
        assert_eq!(exact.canonical_bytes + exact.memo_bytes, limit);
        let mut short = HistoricalSourceAudit::new(Some(&package), 42).unwrap();
        short.memo_bytes = limit - charge + 1;
        short.check(&world, &registry, "study").unwrap();
        short.check(&world, &registry, "study").unwrap();
        assert_eq!(
            short.canonical.capacity(),
            0,
            "precharge rejects before retained allocation"
        );
        assert_eq!(short.canonical_bytes, 0);
        assert_eq!(
            short.counts.source_reconstructions, 2,
            "optional cache refusal recomputes the same complete bounded proof"
        );
        assert!(short
            .check(&changed_bounds(&world), &registry, "study")
            .is_err());
        assert_eq!(short.counts.source_reconstructions, 3);
    }

    #[test]
    fn canonical_source_cache_counter_and_timing_control_compares_same_lawful_workload() {
        const ROOTS: usize = 64;
        let package = super::super::tests::package();
        let (world, registry) = roots(&package);
        let samples: Vec<_> = (0..ROOTS)
            .map(|n| {
                let mut next = (*world).clone();
                next.set_gravity(RegionId(1), [0, (n % 3) as i32, 0])
                    .unwrap();
                assert!(next.historical_checkpoint_valid());
                next
            })
            .collect();
        let mut uncached = HistoricalSourceAudit::new(Some(&package), 42).unwrap();
        uncached.memo_bytes = tor_simulation::checkpoint::ARCHIVE_BYTE_LIMIT;
        let start = std::time::Instant::now();
        for world in &samples {
            uncached.check(world, &registry, "study").unwrap();
        }
        let uncached_time = start.elapsed();
        let mut cached = HistoricalSourceAudit::new(Some(&package), 42).unwrap();
        let start = std::time::Instant::now();
        for world in &samples {
            cached.check(world, &registry, "study").unwrap();
        }
        let cached_time = start.elapsed();
        assert_eq!(
            (
                uncached.counts.source_reconstructions,
                cached.counts.source_reconstructions
            ),
            (ROOTS, 1)
        );
        assert_eq!(
            uncached.counts.scope_comparisons,
            cached.counts.scope_comparisons
        );
        assert_eq!(uncached.counts.extent_checks, cached.counts.extent_checks);
        eprintln!("historical host same-workload control: roots={ROOTS}, source_reconstructions={}->{}, uncached_us={}, cached_us={}, proofs={}",
            uncached.counts.source_reconstructions,cached.counts.source_reconstructions,
            uncached_time.as_micros(),cached_time.as_micros(),cached.counts.extent_checks);
    }

    #[test]
    fn canonical_source_cache_reuses_maximal_generated_source_free_geometry_not_world_authority() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/rogue-geometry");
        let mut manifest = resolve_source(&root).unwrap().manifest;
        let family = manifest.procedural_worlds.get_mut("dungeon").unwrap();
        family.grid = [8, 8];
        family.size = [32, 32, 2];
        family.check().unwrap();
        manifest
            .generation_recipes
            .get_mut("exploration")
            .unwrap()
            .stages[2]
            .operation = crate::generation_recipe::Operation::Rooms {
            width: [30, 30],
            height: [30, 30],
        };
        let package = Package::from_parts(
            manifest,
            vec![],
            tor_simulation::rules::DEFAULT_RULESET_TOML,
        )
        .unwrap();
        package.check().unwrap();
        let (scope, world) = package.canonical_scope_geometry("dungeon/1", 42).unwrap();
        assert_eq!(scope.members.len(), 64);
        assert!(world.historical_checkpoint_valid());
        let mut registry = Registry::default();
        registry.register_archive_checked(scope).unwrap();
        let encoded = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
        let retained = crate::storage::schema::scenario::decode_content_package(&encoded).unwrap();
        assert!(retained.directory.is_none());
        let required_regions = world.loaded_regions().count();
        let samples: Vec<_> = (0..8)
            .map(|n| {
                let mut next = world.clone();
                next.set_gravity(RegionId(1), [0, 0, n - 4]).unwrap();
                assert!(next.historical_checkpoint_valid());
                Shared::new(next)
            })
            .collect();
        let registry = Shared::new(registry);
        let mut audit = HistoricalSourceAudit::new(Some(&retained), 42).unwrap();
        let start = std::time::Instant::now();
        for world in &samples {
            audit.check_shared(world, &registry, "dungeon/1").unwrap();
        }
        let elapsed = start.elapsed();
        assert_eq!(audit.counts.source_reconstructions, 1);
        assert_eq!(audit.counts.scope_comparisons, samples.len());
        assert_eq!(audit.counts.extent_checks, samples.len() * required_regions);
        assert_eq!(audit.canonical_bytes, owned_cache_bytes(&audit));
        eprintln!("historical host maximal generated source-free: members=64, roots={}, reconstruct=1, extent_proofs={}, elapsed_us={}, retained_bytes={}",
            samples.len(),audit.counts.extent_checks,elapsed.as_micros(),audit.canonical_bytes+audit.memo_bytes);
    }
}
