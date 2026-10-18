//! Source-pinned birth stock. Selection is independent of ordinary population
//! streams and ownership remains the ordinary item store after publication.
use super::*;

pub const MAX_CARRIED_ITEM_POOLS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CarryChance {
    pub numerator: u32,
    pub denominator: u32,
}
impl<'de> Deserialize<'de> for CarryChance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            numerator: u32,
            denominator: u32,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            numerator: fields.numerator,
            denominator: fields.denominator,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CarriedLoot {
    pub version: u32,
    pub pool: String,
    pub chance: CarryChance,
}
impl<'de> Deserialize<'de> for CarriedLoot {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            version: u32,
            pool: String,
            chance: CarryChance,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            version: fields.version,
            pool: fields.pool,
            chance: fields.chance,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedCarriedItemPool {
    pub version: u32,
    pub choices: Vec<PreparedItemChoice>,
}
impl<'de> Deserialize<'de> for PreparedCarriedItemPool {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            version: u32,
            #[serde(deserialize_with = "read_choices")]
            choices: Vec<PreparedItemChoice>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            version: fields.version,
            choices: fields.choices,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CarriedLootDecision {
    NotCarried,
    Carried {
        content: String,
        quantity: u64,
        item: ItemId,
    },
}

impl<'de> Deserialize<'de> for CarriedLootDecision {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            NotCarried {},
            Carried {
                content: String,
                quantity: u64,
                item: ItemId,
            },
        }
        Ok(match Fields::deserialize(d)? {
            Fields::NotCarried {} => Self::NotCarried,
            Fields::Carried {
                content,
                quantity,
                item,
            } => Self::Carried {
                content,
                quantity,
                item,
            },
        })
    }
}

pub(super) fn read_pools<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, PreparedCarriedItemPool>, D::Error> {
    bounded_map::<_, _, _, MAX_CARRIED_ITEM_POOLS>(d)
}

impl PreparedItemChoice {
    pub(super) fn valid(&self, game: &Game) -> bool {
        let low = self.quantity.resolve(Some(1));
        let high = self.quantity.resolve(Some(u32::MAX));
        low.is_ok()
            && high.is_ok()
            && self.spec.valid()
            && (self.spec.stackable
                || low.is_ok_and(|range| range.maximum == 1)
                    && high.is_ok_and(|range| range.maximum == 1))
            && self.spec.validate_rules(game.content.rules()).is_ok()
    }
}
impl PreparedCarriedItemPool {
    pub(super) fn valid(&self, game: &Game) -> bool {
        let mut names = BTreeSet::new();
        let mut total = 0u32;
        self.version == 1
            && !self.choices.is_empty()
            && self.choices.len() <= MAX_CHOICES
            && self.choices.iter().all(|choice| {
                label(&choice.content)
                    && names.insert(&choice.content)
                    && choice.weight > 0
                    && total.checked_add(choice.weight).is_some_and(|sum| {
                        total = sum;
                        true
                    })
                    && choice.valid(game)
            })
    }
}
impl CarriedLoot {
    pub(super) fn valid(&self, pools: &BTreeMap<String, PreparedCarriedItemPool>) -> bool {
        self.version == 1
            && label(&self.pool)
            && pools.contains_key(&self.pool)
            && self.chance.valid()
    }
}

impl CarryChance {
    pub(super) fn valid(self) -> bool {
        self.denominator > 0 && self.numerator <= self.denominator
    }
    pub(super) fn succeeds(self, rng: &mut u64) -> bool {
        self.numerator != 0
            && (self.numerator == self.denominator
                || index(rng, self.denominator as usize) < self.numerator as usize)
    }
}

#[derive(Clone, Copy)]
pub(super) struct SelectedBirthStock<'a> {
    pub content: &'a str,
    pub spec: &'a ItemSpec,
    pub quantity: u64,
}

/// A new domain leaves every existing count/content/placement/build root intact.
/// Pool identity is used only for item content, never the chance branch.
pub(super) struct BirthStockContext<'a> {
    pub seed: u64,
    pub scope: &'a str,
    pub profile_id: &'a str,
    pub profile: &'a PopulationProfile,
    pub stage: &'a PopulationStage,
    pub ordinal: u64,
    pub depth: Option<u32>,
}
impl BirthStockContext<'_> {
    pub(super) fn stream(
        &self,
        choice: &PreparedActorChoice,
        policy: &CarriedLoot,
        pool: Option<(&str, &PreparedCarriedItemPool)>,
    ) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        let mut add = |bytes: &[u8]| {
            for byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
                hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
            }
        };
        add(b"tor-spawn-carried-loot-v1");
        add(&self.seed.to_le_bytes());
        add(self.scope.as_bytes());
        add(self.profile_id.as_bytes());
        add(&self.profile.version.to_le_bytes());
        add(self.stage.id.as_bytes());
        add(&self.stage.version.to_le_bytes());
        add(&self.ordinal.to_le_bytes());
        add(choice.content.as_bytes());
        add(&policy.version.to_le_bytes());
        match pool {
            None => add(b"chance"),
            Some((id, pool)) => {
                add(b"item-content");
                add(id.as_bytes());
                add(&pool.version.to_le_bytes());
            }
        }
        hash
    }

    fn quantity_stream(
        &self,
        choice: &PreparedActorChoice,
        policy: &CarriedLoot,
        pool: &PreparedCarriedItemPool,
        item: &PreparedItemChoice,
    ) -> u64 {
        // Keep the historical chance/content roots byte-for-byte. Extending
        // the content root binds actor, policy and pool identities independently
        // from the selected item's separate quantity domain.
        let mut hash = self.stream(choice, policy, Some((&policy.pool, pool)));
        for bytes in [b"item-quantity".as_slice(), item.content.as_bytes()] {
            for byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
                hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
            }
        }
        hash
    }

    /// Pure local selection. The actor choice distinguishes no policy from a
    /// chance miss, and the caller attaches the ordinary allocated item ID.
    pub fn select<'a>(
        &self,
        choice: &PreparedActorChoice,
        pools: &'a BTreeMap<String, PreparedCarriedItemPool>,
    ) -> Result<Option<SelectedBirthStock<'a>>, GameError> {
        let Some(policy) = choice.carried_loot.as_ref() else {
            return Ok(None);
        };
        let pool = &pools[&policy.pool];
        // Missing birth context and arithmetic failure cannot be hidden by a
        // chance miss or a different weighted draw.
        for item in &pool.choices {
            item.quantity
                .resolve(self.depth)
                .map_err(|_| GameError::InvalidQuantity)?;
        }
        let mut rng = self.stream(choice, policy, None);
        if !policy.chance.succeeds(&mut rng) {
            return Ok(None);
        }
        let mut rng = self.stream(choice, policy, Some((&policy.pool, pool)));
        let item = weighted_choice(&mut rng, &pool.choices, |item| item.weight);
        let mut quantity_rng = self.quantity_stream(choice, policy, pool, item);
        let quantity = item
            .quantity
            .resolve(self.depth)
            .map_err(|_| GameError::InvalidQuantity)?
            .sample(&mut quantity_rng)?;
        Ok(Some(SelectedBirthStock {
            content: &item.content,
            spec: &item.spec,
            quantity,
        }))
    }
}

impl PopulationEntity {
    /// Exhaustive origin projection shared by runtime, reservation and archive
    /// auditors. These are historical births, not current ownership assertions.
    pub fn actor_birth(&self) -> Option<ActorId> {
        match self {
            Self::Actor { id, .. } => Some(*id),
            Self::Item { .. } => None,
        }
    }
    pub fn item_birth(&self) -> Option<ItemId> {
        match self {
            Self::Actor {
                carried_loot: Some(CarriedLootDecision::Carried { item, .. }),
                ..
            } => Some(*item),
            Self::Actor { .. } => None,
            Self::Item { id, .. } => Some(*id),
        }
    }
    fn policy_decisions(&self) -> usize {
        match self {
            Self::Actor {
                carried_loot: Some(_),
                ..
            } => 1,
            Self::Actor {
                carried_loot: None, ..
            }
            | Self::Item { .. } => 0,
        }
    }
}
impl PlacementDecision {
    pub fn decision_count(&self) -> usize {
        1 + match self {
            Self::Placed { entity, .. } => entity.policy_decisions(),
            Self::Skipped { .. } => 0,
        }
    }
}
impl StageDecision {
    pub fn decision_count(&self) -> usize {
        1 + match &self.result {
            StageResult::Skipped { .. } => 0,
            StageResult::RoomsEvaluated { rooms } => rooms
                .iter()
                .map(|room| {
                    1 + match &room.result {
                        RoomResult::ChanceMiss => 0,
                        RoomResult::Attempt { decision } => decision.decision_count(),
                    }
                })
                .sum(),
            StageResult::Evaluated { attempts, .. } => {
                attempts.iter().map(PlacementDecision::decision_count).sum()
            }
        }
    }
}
impl PopulationStage {
    pub(super) fn maximum_decisions(&self, rooms: usize) -> Option<usize> {
        if matches!(self.population, PopulationContent::RoomItems { .. }) {
            return rooms.checked_mul(2)?.checked_add(1);
        }
        let with_policy = match &self.population {
            PopulationContent::Actors { bands, .. } => bands.iter().any(|band| {
                band.choices
                    .iter()
                    .any(|choice| choice.carried_loot.is_some())
            }),
            PopulationContent::Items { .. } | PopulationContent::RoomItems { .. } => false,
        };
        usize::from(self.count[1])
            .checked_mul(1 + usize::from(with_policy))?
            .checked_add(1)
    }
}
