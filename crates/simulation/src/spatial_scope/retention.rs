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
    },
    Decoded(codec::DeferredPool),
    InvalidCapture,
}
impl Default for PoolState {
    fn default() -> Self {
        Self::Captured {
            directories: vec![],
            references: vec![],
        }
    }
}
impl PoolState {
    pub(crate) fn capture(&mut self, root: &Shared<Registry>) -> usize {
        let Self::Captured {
            directories,
            references,
        } = self
        else {
            // Infallible capture cannot grant a complete archive from an
            // unbound decoded pool. The poisoned state serializer rejects the
            // complete archive; root placeholder alone is NOT authority.
            *self = Self::InvalidCapture;
            return 0;
        };
        if references.len() >= crate::checkpoint::MAX_CHECKPOINT_ROOTS {
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
            Self::Decoded(pool) => pool.candidate().finalize(roots),
            Self::InvalidCapture => Err(PoolError::Structure),
        }
    }
}
impl Serialize for PoolState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Captured {
                directories,
                references,
            } => {
                let validated = codec::capture_pool(directories, references.iter().copied())
                    .map_err(|_| serde::ser::Error::custom("invalid spatial capture pool"))?;
                EncodedPool(&validated).serialize(s)
            }
            Self::Decoded(_) | Self::InvalidCapture => Err(serde::ser::Error::custom(
                "spatial root binding required before complete capture",
            )),
        }
    }
}
impl<'de> Deserialize<'de> for PoolState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        codec::DeferredSeed.deserialize(d).map(Self::Decoded)
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
