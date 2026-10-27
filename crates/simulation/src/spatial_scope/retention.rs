//! Native structural retention scaffold, not canonical source certification.
use super::registry::{Budget, Error, Registry, Scope};
impl crate::Game {
    pub fn spatial_registry(&self) -> &Registry {
        &self.spatial_registry
    }
    pub fn register_spatial_scope(&mut self, scope: Scope, budget: &Budget) -> Result<(), Error> {
        let mut next = self.spatial_registry.clone();
        next.register(scope, budget)?;
        // Admission rejects an intrinsically unencodable root before publication;
        // the complete history archive still performs its own aggregate check.
        super::codec::capture_pool(std::slice::from_ref(&next), std::iter::once(0)).map_err(
            |error| match error {
                super::codec::PoolError::Structure => Error::Structure,
                super::codec::PoolError::Capacity => Error::Capacity,
            },
        )?;
        self.spatial_registry = next;
        Ok(())
    }
}

use super::codec::{self, EncodedPool, PoolError, ValidatedPool};
use serde::de::DeserializeSeed;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tor_world::Shared;

pub(crate) enum PoolState {
    Captured {
        directories: Vec<Shared<Registry>>,
        references: Vec<usize>,
        snapshot_references: usize,
    },
    Decoded {
        pool: codec::DeferredPool,
        // Only canonical serialization may use this witness. Restoration always
        // revalidates its actual root references against the original meter.
        serialization: Option<ValidatedPool>,
    },
    InvalidCapture,
}
impl Default for PoolState {
    fn default() -> Self {
        Self::Captured {
            directories: vec![],
            references: vec![],
            snapshot_references: 0,
        }
    }
}
impl PoolState {
    pub(crate) fn capture_snapshot(&mut self, root: &Shared<Registry>) -> usize {
        let Self::Captured {
            snapshot_references,
            ..
        } = self
        else {
            *self = Self::InvalidCapture;
            return 0;
        };
        if *snapshot_references >= crate::checkpoint::MAX_CHECKPOINT_ROOTS {
            *self = Self::InvalidCapture;
            return 0;
        }
        *snapshot_references += 1;
        self.capture(root)
    }
    pub(crate) fn capture(&mut self, root: &Shared<Registry>) -> usize {
        let Self::Captured {
            directories,
            references,
            ..
        } = self
        else {
            // Infallible capture cannot grant a complete archive from an
            // unbound decoded pool. The poisoned state serializer rejects the
            // complete archive; root placeholder alone is NOT authority.
            *self = Self::InvalidCapture;
            return 0;
        };
        if references.len() >= super::codec::MAX_ROOT_REFERENCES {
            *self = Self::InvalidCapture;
            return 0;
        }
        let index = directories
            .iter()
            .position(|r| r.shares_storage(root) || r == root)
            .unwrap_or_else(|| {
                directories.push(root.clone());
                directories.len() - 1
            });
        references.push(index);
        index
    }
    pub(crate) fn bind(
        &self,
        roots: impl ExactSizeIterator<Item = usize>,
    ) -> Result<ValidatedPool, PoolError> {
        match self {
            Self::Captured { directories, .. } => codec::capture_pool(directories, roots),
            Self::Decoded { pool, .. } => pool.candidate().finalize(roots),
            Self::InvalidCapture => Err(PoolError::Structure),
        }
    }
}
impl PoolState {
    /// Outer admission checks must revoke earlier serialization authority too.
    /// Failure may occur before the codec receives a replacement root iterator.
    pub(crate) fn clear_serialization_witness(&mut self) {
        if let Self::Decoded { serialization, .. } = self {
            *serialization = None;
        }
    }
    pub(crate) fn prepare_serialization(
        &mut self,
        roots: impl ExactSizeIterator<Item = usize>,
    ) -> Result<(), PoolError> {
        let Self::Decoded {
            pool,
            serialization,
        } = self
        else {
            return Err(PoolError::Structure);
        };
        // A failed rebind must never leave a previous serialization witness.
        *serialization = None;
        *serialization = Some(pool.candidate().finalize(roots)?);
        Ok(())
    }
}
impl Serialize for PoolState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Captured {
                directories,
                references,
                ..
            } => {
                let validated = codec::capture_pool(directories, references.iter().copied())
                    .map_err(|_| serde::ser::Error::custom("invalid spatial capture pool"))?;
                EncodedPool(&validated).serialize(s)
            }
            Self::Decoded {
                serialization: Some(pool),
                ..
            } => EncodedPool(pool).serialize(s),
            Self::Decoded {
                serialization: None,
                ..
            }
            | Self::InvalidCapture => Err(serde::ser::Error::custom(
                "spatial root binding required before complete capture",
            )),
        }
    }
}
impl<'de> Deserialize<'de> for PoolState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        codec::DeferredSeed
            .deserialize(d)
            .map(|pool| Self::Decoded {
                pool,
                serialization: None,
            })
    }
}
pub(crate) mod root_index {
    use super::*;
    pub fn serialize<S: Serializer>(index: &usize, s: S) -> Result<S::Ok, S::Error> {
        u32::try_from(*index)
            .map_err(|_| serde::ser::Error::custom("spatial root index overflow"))?
            .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<usize, D::Error> {
        usize::try_from(u32::deserialize(d)?)
            .map_err(|_| serde::de::Error::custom("spatial root index overflow"))
    }
}
#[cfg(test)]
mod tests;
