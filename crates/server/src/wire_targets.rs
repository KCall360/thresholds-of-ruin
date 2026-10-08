//! Stable target identity needs only saved privacy material and private IDs.
//! It never consults live world state, so receipts survive target removal.
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use tor_protocol::{ActorTarget, DoorTarget, ItemTarget};
use tor_simulation::{ActorId, ItemId};
use uuid::Uuid;

/// Saved-game and observer scope for interaction identities. Derivation alone
/// grants no permission; fresh requests must resolve against disclosed entities.
#[derive(Clone)]
pub struct TargetScope {
    prefix: Hmac<Sha256>,
    observer: ActorId,
}

impl TargetScope {
    /// The salt belongs to the saved archive; the observer is a private domain ID.
    pub fn new(salt: Uuid, observer: ActorId) -> Self {
        let mut prefix = Hmac::<Sha256>::new_from_slice(salt.as_bytes())
            .expect("HMAC accepts the fixed UUID key size");
        prefix.update(b"tor-interaction-target-v1\0");
        prefix.update(&observer.0.to_le_bytes());
        Self { prefix, observer }
    }

    pub fn observer(&self) -> ActorId {
        self.observer
    }

    fn digest(&self, kind: u8, identity: u64) -> [u8; 32] {
        let mut mac = self.prefix.clone();
        mac.update(&[kind]);
        mac.update(&identity.to_le_bytes());
        mac.finalize().into_bytes().into()
    }

    pub fn actor(&self, identity: ActorId) -> ActorTarget {
        ActorTarget::from_digest(self.digest(b'a', identity.0))
    }

    pub fn item(&self, identity: ItemId) -> ItemTarget {
        ItemTarget::from_digest(self.digest(b'i', identity.0))
    }

    pub fn door(&self, identity: u64) -> DoorTarget {
        DoorTarget::from_digest(self.digest(b'd', identity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Independent Python stdlib hmac/SHA-256 calculation of the declared byte layout.
    const VECTOR: &str = "a_7a23df737ca869f84c95d05f4f08f2bbc00d21675a207b468be8d127ebf821f8";

    #[test]
    fn saved_scope_reconstructs_identity_without_a_live_target_or_rng() {
        let salt = Uuid::parse_str("00112233-4455-4677-8899-aabbccddeeff").unwrap();
        let first = TargetScope::new(salt, ActorId(7));
        let restored = TargetScope::new(
            Uuid::parse_str("00112233-4455-4677-8899-AABBCCDDEEFF").unwrap(),
            ActorId(7),
        );
        for identity in [0, 1, 255, u64::MAX] {
            assert_eq!(
                first.actor(ActorId(identity)),
                restored.actor(ActorId(identity))
            );
            assert_eq!(
                first.item(ItemId(identity)),
                restored.item(ItemId(identity))
            );
            assert_eq!(first.door(identity), restored.door(identity));
        }
        // Pin the namespace and fixed-width input layout independently of world state.
        assert_eq!(first.actor(ActorId(u64::MAX)).to_string(), VECTOR);
    }

    #[test]
    fn identities_separate_saves_observers_entities_and_entity_kinds() {
        let salt = Uuid::from_u128(1);
        let scope = TargetScope::new(salt, ActorId(7));
        let others = [
            TargetScope::new(Uuid::from_u128(2), ActorId(7)),
            TargetScope::new(salt, ActorId(8)),
        ];
        for other in others {
            assert_ne!(scope.actor(ActorId(1)), other.actor(ActorId(1)));
            assert_ne!(scope.item(ItemId(1)), other.item(ItemId(1)));
            assert_ne!(scope.door(1), other.door(1));
        }
        assert_ne!(scope.actor(ActorId(1)), scope.actor(ActorId(2)));
        assert_ne!(scope.item(ItemId(1)), scope.item(ItemId(2)));
        assert_ne!(scope.door(1), scope.door(2));
        let actor = scope.actor(ActorId(1)).to_string();
        let item = scope.item(ItemId(1)).to_string();
        let door = scope.door(1).to_string();
        assert_ne!(&actor[2..], &item[2..]);
        assert_ne!(&actor[2..], &door[2..]);
        assert_ne!(&item[2..], &door[2..]);
    }
}
