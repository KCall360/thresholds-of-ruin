//! Standalone experimental codec scaffold: no Game/save field or format migration.
//! Caller limits and validated-only encoding do not certify canonical geometry.
use super::registry::Scope;
use serde::{de::DeserializeSeed, Deserializer, Serialize, Serializer};
use tor_world::Shared;

#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    pub charge_bytes: u64,
    /// Actual root references reserved by the enclosing owner, NOT an assumed257.
    pub root_references: u32,
    /// Explicit caller directory bound, independently enforced while reading.
    pub directories: u32,
}
/// A decoded candidate has no install/encoding authority until roots are checked.
/// Fields are private; production obtains one only through PoolSeed.
#[derive(Debug)]
pub struct DecodedPool {
    scopes: Vec<Shared<Scope>>,
    /// Entries ordered by referenced scope identity, not pool allocation index.
    directories: Vec<Vec<u32>>,
    limits: DecodeLimits,
}
/// Validated immutable payload plus the exact supplied root references.
/// There is deliberately no unchecked public constructor or mutable accessor.
#[derive(Debug)]
pub struct ValidatedPool {
    scopes: Vec<Shared<Scope>>,
    directories: Vec<Vec<u32>>,
    roots: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolError {
    Structure,
    Capacity,
}
impl DecodedPool {
    /// Consumes the candidate. Checks the reserved count, all indices, semantic
    /// aliases, and reference completeness before copying roots or returning
    /// authority. u32 conversions for future capture writers must be checked.
    pub fn validate_roots(self, roots: &[u32]) -> Result<ValidatedPool, PoolError> {
        // First-red scaffold: no successful validation authority yet.
        let _ = (self.scopes, self.directories, self.limits, roots);
        Err(PoolError::Structure)
    }
}
/// Persistence wire adapter; never adds Serialize to simulation Scope itself.
/// Only validated inputs can be encoded. A future capture owner needs a checked
/// builder routed through the same validation, not a public field constructor.
pub struct EncodedPool<'a>(pub &'a ValidatedPool);
pub struct PoolSeed(pub DecodeLimits);
impl Serialize for EncodedPool<'_> {
    fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        let _ = (&self.0.scopes, &self.0.directories, &self.0.roots);
        Err(serde::ser::Error::custom(
            "spatial pool encoding not implemented",
        ))
    }
}
impl<'de> DeserializeSeed<'de> for PoolSeed {
    type Value = DecodedPool;
    fn deserialize<D: Deserializer<'de>>(self, _deserializer: D) -> Result<Self::Value, D::Error> {
        Err(serde::de::Error::custom(
            "spatial pool decoding not implemented",
        ))
    }
}

#[cfg(test)]
mod tests;
