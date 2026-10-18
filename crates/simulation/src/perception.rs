//! Source-composed perception and one-lifetime object guise authority.
//!
//! Geometry remains physical. An observer's disclosure class is the sole license
//! for identity, targeting, affiliation and public-plan projection.
use crate::{ActorId, Game, GameError, ItemClass};
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{Location, SightCell};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GuiseAppearance {
    pub name: String,
    pub description: String,
    pub class: ItemClass,
    pub asset: Option<String>,
}
impl GuiseAppearance {
    pub fn valid(&self) -> bool {
        let text = |value: &str, nonempty: bool| {
            (!nonempty || !value.trim().is_empty())
                && value.len() <= crate::limits::DISPLAY_NAME_BYTES
                && !value.chars().any(char::is_control)
        };
        text(&self.name, true)
            && text(&self.description, false)
            && self.asset.as_ref().is_none_or(|value| text(value, true))
            && self.class != ItemClass::Corpse
    }
}
impl<'de> Deserialize<'de> for GuiseAppearance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            name: String,
            description: String,
            class: ItemClass,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            asset: Option<String>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            name: fields.name,
            description: fields.description,
            class: fields.class,
            asset: fields.asset,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid guise appearance"));
        }
        Ok(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PerceptionValue {
    Invisible { enabled: bool },
    SeeInvisible { enabled: bool },
    Sight { enabled: bool },
    ObjectGuise { appearance: Option<GuiseAppearance> },
    AmbushFromGuise { enabled: bool },
}
impl<'de> Deserialize<'de> for PerceptionValue {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Invisible {
                enabled: bool,
            },
            SeeInvisible {
                enabled: bool,
            },
            Sight {
                enabled: bool,
            },
            ObjectGuise {
                #[serde(deserialize_with = "crate::bounded::required_option")]
                appearance: Option<GuiseAppearance>,
            },
            AmbushFromGuise {
                enabled: bool,
            },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Invisible { enabled } => Self::Invisible { enabled },
            Fields::SeeInvisible { enabled } => Self::SeeInvisible { enabled },
            Fields::Sight { enabled } => Self::Sight { enabled },
            Fields::ObjectGuise { appearance } => Self::ObjectGuise { appearance },
            Fields::AmbushFromGuise { enabled } => Self::AmbushFromGuise { enabled },
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerceptionCapabilities {
    pub invisible: bool,
    pub see_invisible: bool,
    pub sight: bool,
    pub object_guise: Option<GuiseAppearance>,
    pub ambush_from_guise: bool,
}
impl Default for PerceptionCapabilities {
    fn default() -> Self {
        Self {
            invisible: false,
            see_invisible: false,
            sight: true,
            object_guise: None,
            ambush_from_guise: false,
        }
    }
}
impl PerceptionCapabilities {
    pub(crate) fn from_modifiers<'a>(
        values: impl Iterator<Item = &'a crate::rules::Modifier>,
    ) -> Result<Self, crate::creatures::BuildError> {
        use crate::rules::{ModifierOp, Scope};
        let mut result = Self::default();
        let (mut invisible_off, mut detection_off, mut guise_off, mut ambush_off) =
            (false, false, false, false);
        for modifier in values.filter(|modifier| modifier.scope == Scope::Always) {
            let ModifierOp::Perception { value } = &modifier.op else {
                continue;
            };
            match value {
                PerceptionValue::Invisible { enabled } => {
                    result.invisible |= enabled;
                    invisible_off |= !enabled;
                }
                PerceptionValue::SeeInvisible { enabled } => {
                    result.see_invisible |= enabled;
                    detection_off |= !enabled;
                }
                PerceptionValue::Sight { enabled } => result.sight &= enabled,
                PerceptionValue::ObjectGuise { appearance: None } => guise_off = true,
                PerceptionValue::ObjectGuise {
                    appearance: Some(appearance),
                } => {
                    if !appearance.valid()
                        || result
                            .object_guise
                            .as_ref()
                            .is_some_and(|old| old != appearance)
                    {
                        return Err(crate::creatures::BuildError::InvalidDefinition);
                    }
                    result.object_guise = Some(appearance.clone());
                }
                PerceptionValue::AmbushFromGuise { enabled } => {
                    result.ambush_from_guise |= enabled;
                    ambush_off |= !enabled;
                }
            }
        }
        result.invisible &= !invisible_off;
        result.see_invisible &= !detection_off;
        result.ambush_from_guise &= !ambush_off;
        if guise_off {
            result.object_guise = None;
        }
        Ok(result)
    }
}

/// The actor ID is already a never-reused incarnation. A different reference
/// domain must never be correlated with it by an observer or by its AI memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorReferenceKind {
    Creature,
    Contact,
}
impl<'de> Deserialize<'de> for ActorReferenceKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "creature" => Ok(Self::Creature),
            "contact" => Ok(Self::Contact),
            _ => Err(serde::de::Error::custom("invalid actor reference kind")),
        }
    }
}

/// A historical known-threat handle preserves its disclosure domain. Private
/// actor identity alone never associates contact and identified observations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PerceivedThreat {
    pub actor: ActorId,
    pub reference: ActorReferenceKind,
}
impl<'de> Deserialize<'de> for PerceivedThreat {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: ActorId,
            reference: ActorReferenceKind,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        if fields.actor.0 == 0 || fields.actor.0 == u64::MAX {
            return Err(serde::de::Error::custom("invalid threat actor"));
        }
        Ok(Self {
            actor: fields.actor,
            reference: fields.reference,
        })
    }
}
pub(crate) fn read_threats<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeSet<PerceivedThreat>, D::Error> {
    let values = crate::bounded::Bounded::<
        PerceivedThreat,
        { crate::limits::PUBLIC_AIM_OCCURRENCES },
    >::deserialize(d)?
    .0;
    let mut actors = BTreeSet::new();
    if values.iter().any(|value| !actors.insert(value.actor)) {
        return Err(serde::de::Error::custom("duplicate threat actor"));
    }
    Ok(values.into_iter().collect())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorDisclosure {
    Hidden,
    AnonymousContact,
    ApparentObject,
    Creature,
}
impl ActorDisclosure {
    pub fn reference(self) -> Option<ActorReferenceKind> {
        match self {
            Self::Creature => Some(ActorReferenceKind::Creature),
            Self::AnonymousContact => Some(ActorReferenceKind::Contact),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AmbushState {
    #[default]
    Unacquired,
    Available,
    Reserved {
        origin: crate::IntentionId,
    },
    Spent,
}
impl<'de> Deserialize<'de> for AmbushState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Unacquired {},
            Available {},
            Reserved { origin: crate::IntentionId },
            Spent {},
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Unacquired {} => Self::Unacquired,
            Fields::Available {} => Self::Available,
            Fields::Reserved { origin } if origin.0 > 0 && origin.0 < u64::MAX => {
                Self::Reserved { origin }
            }
            Fields::Reserved { .. } => {
                return Err(serde::de::Error::custom("invalid ambush owner"))
            }
            Fields::Spent {} => Self::Spent,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuiseState {
    entered: bool,
    appearance: Option<GuiseAppearance>,
    pub(crate) ambush: AmbushState,
}
impl<'de> Deserialize<'de> for GuiseState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            entered: bool,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            appearance: Option<GuiseAppearance>,
            ambush: AmbushState,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            entered: fields.entered,
            appearance: fields.appearance,
            ambush: fields.ambush,
        };
        if (!value.entered
            && (value.appearance.is_some() || value.ambush != AmbushState::Unacquired))
            || (value.appearance.is_none() && value.ambush == AmbushState::Available)
        {
            return Err(serde::de::Error::custom("inconsistent guise state"));
        }
        Ok(value)
    }
}
impl GuiseState {
    pub(crate) fn appearance(&self) -> Option<&GuiseAppearance> {
        self.appearance.as_ref()
    }
    pub(crate) fn ambush_available(&self) -> bool {
        self.appearance.is_some() && self.ambush == AmbushState::Available
    }
    pub(crate) fn reveal(&mut self) {
        if !self.entered {
            return;
        }
        self.appearance = None;
        if !matches!(self.ambush, AmbushState::Reserved { .. }) {
            self.ambush = AmbushState::Spent;
        }
    }
    pub(crate) fn claim_valid(&self, pending: Option<&crate::combat::Preparation>) -> bool {
        let claimed = pending.filter(|pending| {
            pending
                .plan
                .as_ability()
                .is_some_and(|plan| plan.ambush.is_some())
        });
        match self.ambush {
            AmbushState::Reserved { origin } => claimed.is_some_and(|pending| {
                pending.origin == Some(origin) && pending.next_strike() == Some(0)
            }),
            AmbushState::Spent => {
                claimed.is_none_or(|pending| pending.next_strike().is_some_and(|next| next > 0))
            }
            _ => claimed.is_none(),
        }
    }
    pub(crate) fn valid_for(
        &self,
        capabilities: &PerceptionCapabilities,
        pending: Option<&crate::combat::Preparation>,
        alive: bool,
    ) -> bool {
        self.claim_valid(pending)
            && self.appearance.as_ref().is_none_or(|appearance| {
                alive && capabilities.object_guise.as_ref() == Some(appearance)
            })
            && (self.ambush != AmbushState::Available || capabilities.ambush_from_guise)
            && (self.entered || capabilities.object_guise.is_none())
    }
    pub(crate) fn retire_claim(&mut self, pending: Option<&crate::combat::Preparation>) {
        if let AmbushState::Reserved { origin } = self.ambush {
            if pending.is_none_or(|pending| {
                pending.origin != Some(origin)
                    || pending
                        .plan
                        .as_ability()
                        .is_none_or(|plan| plan.ambush.is_none())
                    || pending.next_strike() != Some(0)
            }) {
                self.ambush = AmbushState::Spent;
            }
        }
    }
    /// First legitimate source acquisition, including an ordinary player purchase,
    /// enters exactly once. A later loss/respec/regrant cannot recreate that entry.
    pub(crate) fn reconcile(&mut self, capabilities: &PerceptionCapabilities, moved: bool) {
        if !self.entered {
            if let Some(appearance) = &capabilities.object_guise {
                self.entered = true;
                self.appearance = Some(appearance.clone());
            }
        }
        if self.entered && (capabilities.object_guise.is_none() || moved) {
            self.reveal();
        }
        if self.appearance.is_some() {
            match (&self.ambush, capabilities.ambush_from_guise) {
                (AmbushState::Unacquired, true) => self.ambush = AmbushState::Available,
                (AmbushState::Available, false) => self.ambush = AmbushState::Spent,
                _ => {}
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApparentObjectView {
    /// Private resolved identity; adapters emit only the apparent-object domain.
    pub subject: ActorId,
    pub location: Location,
    pub appearance: GuiseAppearance,
}

pub(crate) struct PerceivedActor {
    pub disclosure: ActorDisclosure,
    pub body: crate::actor_store::BodyCells,
}
struct DisclosureInputs<'a> {
    observer: ActorId,
    scene: &'a [SightCell],
    eyes: &'a BTreeSet<Location>,
    capabilities: &'a PerceptionCapabilities,
    placement: Option<crate::abilities::Placement>,
    contact: &'a OnceCell<BTreeSet<Location>>,
}
pub(crate) type PerceptionProjection = BTreeMap<ActorId, PerceivedActor>;

impl crate::Actor {
    pub(crate) fn perception(&self) -> PerceptionCapabilities {
        self.combat
            .as_ref()
            .map_or_else(PerceptionCapabilities::default, |combat| {
                combat.creature().derived().perception.clone()
            })
    }
}
impl Game {
    pub(crate) fn validate_apparent_touch(
        &self,
        observer: ActorId,
        subject: ActorId,
        quantity: Option<u64>,
    ) -> Result<(), GameError> {
        if quantity.is_some_and(|quantity| quantity != 1) {
            return Err(GameError::InvalidQuantity);
        }
        let (scene, sight) = self.scene_snapshot(observer)?;
        let disclosed = self
            .perception_projection(observer, &scene, &sight)?
            .get(&subject)
            .is_some_and(|row| row.disclosure == ActorDisclosure::ApparentObject);
        if !disclosed || !self.melee_reachable_in_scene(observer, subject, &scene) {
            return Err(GameError::ItemUnavailable);
        }
        Ok(())
    }
    pub(crate) fn reveal_guise(&mut self, subject: ActorId) -> Result<(), GameError> {
        let mut outcome = self.actor_outcome(subject)?;
        outcome.state.guise.reveal();
        self.apply_outcome(outcome)
    }
    pub(crate) fn reveal_adjacent_guises(&mut self, observer: ActorId) -> Result<(), GameError> {
        let awareness = self.local_awareness(observer);
        let cells = awareness
            .iter()
            .filter(|cell| cell.physical)
            .map(|cell| cell.location)
            .collect();
        let targets: Vec<_> = self
            .actors
            .perceived(&self.world, &cells)
            .into_iter()
            .filter(|(subject, _)| {
                *subject != observer
                    && self.actors[subject].guise.appearance().is_some()
                    && self.melee_reachable_in_scene(observer, *subject, &awareness)
            })
            .map(|(subject, _)| subject)
            .collect();
        for subject in targets {
            self.reveal_guise(subject)?;
        }
        Ok(())
    }
    pub fn actor_perception(&self, id: ActorId) -> Option<PerceptionCapabilities> {
        self.actors.get(&id).map(crate::Actor::perception)
    }

    /// Exact visual eligibility, independent of tactile/stair disclosure. A seen
    /// object is not a license to target the creature concealed by its guise.
    pub(crate) fn actor_visible_in_sight(
        &self,
        observer: ActorId,
        subject: ActorId,
        sight: &[SightCell],
    ) -> bool {
        let Some(owner) = self.actors.get(&observer) else {
            return false;
        };
        let Some(target) = self.actors.get(&subject) else {
            return false;
        };
        let capabilities = owner.perception();
        if !capabilities.sight
            || !target.alive()
            || target.guise.appearance().is_some()
            || (target.perception().invisible && !capabilities.see_invisible)
        {
            return false;
        }
        let Some(body) = self.body_cells(target.location, target.orientation, &target.body) else {
            return false;
        };
        let cells: BTreeSet<_> = body.into_iter().map(|(at, _)| at).collect();
        sight.iter().any(|cell| cells.contains(&cell.location))
    }
    pub(crate) fn genuinely_sees_actor_in_sight(
        &self,
        observer: ActorId,
        subject: ActorId,
        range: u16,
        sight: &[SightCell],
    ) -> bool {
        if !self.actor_visible_in_sight(observer, subject, sight) {
            return false;
        }
        let target = &self.actors[&subject];
        let Some(body) = self.body_cells(target.location, target.orientation, &target.body) else {
            return false;
        };
        let cells: BTreeSet<_> = body.into_iter().map(|(at, _)| at).collect();
        sight.iter().any(|cell| {
            cells.contains(&cell.location)
                && cell.offset.x.unsigned_abs()
                    + cell.offset.y.unsigned_abs()
                    + cell.offset.z.unsigned_abs()
                    <= u32::from(range)
        })
    }
    pub(crate) fn genuinely_sees_actor(
        &self,
        observer: ActorId,
        subject: ActorId,
        range: u16,
    ) -> bool {
        self.scene_snapshot(observer).is_ok_and(|(_, sight)| {
            self.genuinely_sees_actor_in_sight(observer, subject, range, &sight)
        })
    }
    pub(crate) fn perceived_hostile(
        &self,
        observer: ActorId,
        subject: ActorId,
        disclosure: ActorDisclosure,
    ) -> bool {
        let Some(reference) = disclosure.reference() else {
            return false;
        };
        observer != subject
            && ((reference == ActorReferenceKind::Creature
                && self.faction_hostile(observer, subject))
                || self
                    .actors
                    .get(&observer)
                    .and_then(|actor| actor.engagement_at(self.actor_clock(observer)))
                    .is_some_and(|engagement| {
                        engagement.target == subject && engagement.reference == reference
                    }))
    }

    #[cfg(test)]
    pub(crate) fn classified_actors(
        &self,
        observer: ActorId,
    ) -> Result<PerceptionProjection, GameError> {
        let (scene, sight) = self.scene_snapshot(observer)?;
        self.perception_projection(observer, &scene, &sight)
    }

    /// A bounded single-subject query. It shares the projection classifier and
    /// never enumerates actors or resolves a second world scene.
    pub(crate) fn actor_disclosure(
        &self,
        observer: ActorId,
        subject: ActorId,
    ) -> Result<ActorDisclosure, GameError> {
        let (scene, sight) = self.scene_snapshot(observer)?;
        self.actor_disclosure_in_scene(observer, subject, &scene, &sight)
    }
    pub(crate) fn actor_disclosure_in_scene(
        &self,
        observer: ActorId,
        subject: ActorId,
        scene: &[SightCell],
        sight: &[SightCell],
    ) -> Result<ActorDisclosure, GameError> {
        let owner = self.actors.get(&observer).ok_or(GameError::UnknownActor)?;
        let Some(target) = self.actors.get(&subject) else {
            return Ok(ActorDisclosure::Hidden);
        };
        let Some(body) = self.body_cells(target.location, target.orientation, &target.body) else {
            return Ok(ActorDisclosure::Hidden);
        };
        if !body
            .iter()
            .any(|(at, _)| scene.iter().any(|cell| cell.location == *at))
        {
            return Ok(ActorDisclosure::Hidden);
        }
        let eyes = sight.iter().map(|cell| cell.location).collect();
        let capabilities = owner.perception();
        let contact = OnceCell::new();
        let inputs = DisclosureInputs {
            observer,
            scene,
            eyes: &eyes,
            capabilities: &capabilities,
            placement: None,
            contact: &contact,
        };
        Ok(self.classify_body(subject, &body, &inputs))
    }

    fn classify_body(
        &self,
        subject: ActorId,
        body: &[(Location, u8)],
        inputs: &DisclosureInputs<'_>,
    ) -> ActorDisclosure {
        let DisclosureInputs {
            observer,
            eyes,
            capabilities,
            ..
        } = *inputs;
        crate::diagnostics::actor_disclosure();
        let state = &self.actors[&subject];
        let eye_visible = capabilities.sight && body.iter().any(|(at, _)| eyes.contains(at));
        if subject == observer {
            ActorDisclosure::Creature
        } else if !state.alive() {
            ActorDisclosure::Hidden
        } else if state.perception().invisible && !(capabilities.see_invisible && eye_visible) {
            if state.combat.as_ref().is_some_and(|combat| combat.hp() > 0)
                && body
                    .iter()
                    .any(|(at, _)| self.contact_locations(inputs).contains(at))
            {
                ActorDisclosure::AnonymousContact
            } else {
                ActorDisclosure::Hidden
            }
        } else if state.guise.appearance().is_some() {
            ActorDisclosure::ApparentObject
        } else {
            ActorDisclosure::Creature
        }
    }

    fn contact_locations<'a>(&self, inputs: &DisclosureInputs<'a>) -> &'a BTreeSet<Location> {
        inputs.contact.get_or_init(|| {
            self.melee_contact_locations_in_scene(inputs.observer, inputs.scene, inputs.placement)
        })
    }

    pub(crate) fn perception_projection(
        &self,
        observer: ActorId,
        scene: &[SightCell],
        sight: &[SightCell],
    ) -> Result<PerceptionProjection, GameError> {
        self.perception_projection_at(observer, scene, sight, None)
    }
    pub(crate) fn perception_projection_at(
        &self,
        observer: ActorId,
        scene: &[SightCell],
        sight: &[SightCell],
        placement: Option<crate::abilities::Placement>,
    ) -> Result<PerceptionProjection, GameError> {
        let owner = self.actors.get(&observer).ok_or(GameError::UnknownActor)?;
        let capabilities = owner.perception();
        let cells: BTreeSet<_> = scene.iter().map(|cell| cell.location).collect();
        let eyes: BTreeSet<_> = sight.iter().map(|cell| cell.location).collect();
        let contact = OnceCell::new();
        crate::diagnostics::perception_projection();
        Ok(self
            .actors
            .perceived(&self.world, &cells)
            .into_iter()
            .map(|(subject, body)| {
                let inputs = DisclosureInputs {
                    observer,
                    scene,
                    eyes: &eyes,
                    capabilities: &capabilities,
                    placement,
                    contact: &contact,
                };
                let disclosure = self.classify_body(subject, &body, &inputs);
                let body = if disclosure == ActorDisclosure::AnonymousContact {
                    tor_world::Shared::new(
                        body.iter()
                            .copied()
                            .filter(|(at, _)| self.contact_locations(&inputs).contains(at))
                            .collect(),
                    )
                } else {
                    body
                };
                (subject, PerceivedActor { disclosure, body })
            })
            .collect())
    }
}

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod tests;
