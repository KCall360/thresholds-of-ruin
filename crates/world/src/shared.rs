//! Deterministic copy-on-write ownership for backend snapshots. Encoding is
//! transparent: sharing is an in-memory implementation detail, not a save format.
use std::{
    borrow::Borrow,
    cmp::Ordering,
    ops::{Deref, DerefMut},
    sync::Arc,
};

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Shared<T>(Arc<T>);

impl<T> Borrow<T> for Shared<T> {
    fn borrow(&self) -> &T {
        &self.0
    }
}

impl<T: Ord> PartialOrd for Shared<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Ord> Ord for Shared<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        (**self).cmp(&**other)
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Default> Default for Shared<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(value))
    }
    /// The value, copied only if something else still shares it.
    pub fn into_inner(self) -> T
    where
        T: Clone,
    {
        Arc::try_unwrap(self.0).unwrap_or_else(|shared| (*shared).clone())
    }
    /// Diagnostic ownership check; never use pointer identity for game decisions.
    pub fn shares_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl<T> Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: Clone> DerefMut for Shared<T> {
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn ordered_shared_values_borrow_content_and_detach_on_edit() {
        let original = Shared::new(vec![1, 2]);
        let pool = BTreeSet::from([original.clone()]);
        let mut borrowed = pool.get(&vec![1, 2]).unwrap().clone();
        assert!(borrowed.shares_storage(&original));
        borrowed.push(3);
        assert_eq!(&*original, &[1, 2]);
        assert_eq!(&*borrowed, &[1, 2, 3]);
        assert!(pool.contains(&vec![1, 2]));
        assert!(!pool.contains(&vec![1, 2, 3]));
        assert_eq!(serde_json::to_vec(&original).unwrap(), b"[1,2]");
    }
}
