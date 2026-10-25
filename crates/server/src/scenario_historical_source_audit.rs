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

/// A restore-pass verifier. Only weak identities and scope labels are retained;
/// expected source Worlds and materialized source definitions die after check.
pub(crate) struct HistoricalSourceAudit<'a> {
    package: Option<&'a Package>,
    seed: u64,
    memo: Vec<Memo>,
    memo_bytes: usize,
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
            .filter(|n| *n <= tor_simulation::checkpoint::ARCHIVE_BYTE_LIMIT)
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
        &self,
        world: &World,
        registry: &Registry,
        scope_id: &str,
    ) -> Result<(), Failure> {
        let package = self.package.ok_or_else(invalid)?;
        let captured = registry
            .scopes()
            .find(|scope| scope.id == scope_id)
            .ok_or_else(invalid)?;
        let (canonical, source) = package.canonical_scope_geometry(scope_id, self.seed)?;
        if captured != &canonical {
            return Err(invalid());
        }
        // Required member/immediate-shell metadata remains source-bound. The
        // source compiler already precharges its bounded geometry construction.
        // Do NOT rescan captured topology/solidity: core validates its retained
        // World authority once per shared root, including chart-only roots.
        for region in source.loaded_regions() {
            let expected = source.region(region).ok_or_else(invalid)?;
            let captured = world.region(region).ok_or_else(invalid)?;
            if captured.bounds != expected.bounds
                || world.is_chamber(region) != source.is_chamber(region)
            {
                return Err(invalid());
            }
        }
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
}
