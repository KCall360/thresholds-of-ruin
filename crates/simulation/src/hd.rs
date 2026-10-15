//! Exact Hit Die counts and zero-based owners, independent of row admission.
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct HdCount(pub u64);
impl HdCount {
    pub const ZERO: Self = Self(0);
    pub fn get(self) -> u64 {
        self.0
    }
    pub fn from_usize(value: usize) -> Result<Self, crate::progression::HdError> {
        u64::try_from(value)
            .map(Self)
            .map_err(|_| crate::progression::HdError::Arithmetic)
    }
    pub fn to_usize(self) -> Result<usize, crate::progression::HdError> {
        usize::try_from(self.0).map_err(|_| crate::progression::HdError::Arithmetic)
    }
    pub fn checked_add(self, value: u64) -> Option<Self> {
        self.0.checked_add(value).map(Self)
    }
    pub fn checked_sub(self, value: Self) -> Option<Self> {
        self.0.checked_sub(value.0).map(Self)
    }
}
impl From<HdCount> for u64 {
    fn from(value: HdCount) -> Self {
        value.0
    }
}
impl std::fmt::Display for HdCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct HdIndex(pub u64);
impl HdIndex {
    pub fn get(self) -> u64 {
        self.0
    }
    pub fn from_usize(value: usize) -> Result<Self, crate::progression::HdError> {
        u64::try_from(value)
            .map(Self)
            .map_err(|_| crate::progression::HdError::Arithmetic)
    }
    pub fn to_usize(self) -> Result<usize, crate::progression::HdError> {
        usize::try_from(self.0).map_err(|_| crate::progression::HdError::Arithmetic)
    }
    pub fn ordinal(self) -> Result<HdCount, crate::progression::HdError> {
        self.0
            .checked_add(1)
            .map(HdCount)
            .ok_or(crate::progression::HdError::Arithmetic)
    }
    pub fn from_ordinal(value: HdCount) -> Option<Self> {
        value.0.checked_sub(1).map(Self)
    }
}
impl From<HdIndex> for u64 {
    fn from(value: HdIndex) -> Self {
        value.0
    }
}
impl std::fmt::Display for HdIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wide_scalar_count_and_owner_boundaries_remain_distinct() {
        for value in [65_535, 65_536, 65_537, (1_u64 << 53) + 1, u64::MAX] {
            let count = HdCount(value);
            let owner = HdIndex::from_ordinal(count).unwrap();
            assert_eq!(owner.get(), value - 1);
            assert_eq!(owner.ordinal(), Ok(count));
            assert_eq!(count.to_string(), value.to_string());
            assert_eq!(
                serde_json::from_str::<HdCount>(&value.to_string()).unwrap(),
                count
            );
        }
        assert_eq!(HdIndex::from_ordinal(HdCount::ZERO), None);
        assert_eq!(
            HdIndex(u64::MAX).ordinal(),
            Err(crate::progression::HdError::Arithmetic)
        );
        // A host index is checked at the point a resident row is addressed.
        for value in [u64::from(u32::MAX), u64::from(u32::MAX) + 1, u64::MAX] {
            assert_eq!(HdIndex(value).to_usize().ok(), usize::try_from(value).ok());
            assert_eq!(HdCount(value).to_usize().ok(), usize::try_from(value).ok());
        }
    }
}
