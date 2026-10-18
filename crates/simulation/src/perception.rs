//! Source-composed perception and one-lifetime object guise authority.
//!
//! Geometry remains physical. An observer's disclosure class is the sole license
//! for identity, targeting, affiliation and public-plan projection.
use crate::{ActorId, Game, GameError, ItemClass};
use serde::{Deserialize, Serialize};
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
        let text = |value: &str, nonempty| {
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
        let value = Self { name: fields.name, description: fields.description, class: fields.class, asset: fields.asset };
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
            Invisible { enabled: bool },
            SeeInvisible { enabled: bool },
            Sight { enabled: bool },
            ObjectGuise {
                #[serde(deserialize_with = "crate::bounded::required_option")]
                appearance: Option<GuiseAppearance>,
            },
            AmbushFromGuise { enabled: bool },
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
        Self { invisible: false, see_invisible: false, sight: true, object_guise: None, ambush_from_guise: false }
    }
}
impl PerceptionCapabilities {
    pub(crate) fn from_modifiers<'a>(values: impl Iterator<Item = &'a crate::rules::Modifier>) -> Result<Self, crate::creatures::BuildError> {
        use crate::rules::{ModifierOp, Scope};
        let mut result = Self::default();
        let (mut invisible_off, mut detection_off, mut guise_off, mut ambush_off) = (false, false, false, false);
        for modifier in values.filter(|modifier| modifier.scope == Scope::Always) {
            let ModifierOp::Perception { value } = &modifier.op else { continue };
            match value {
                PerceptionValue::Invisible { enabled } => { result.invisible |= enabled; invisible_off |= !enabled; },
                PerceptionValue::SeeInvisible { enabled } => { result.see_invisible |= enabled; detection_off |= !enabled; },
                PerceptionValue::Sight { enabled } => result.sight &= enabled,
                PerceptionValue::ObjectGuise { appearance: None } => guise_off = true,
                PerceptionValue::ObjectGuise { appearance: Some(appearance) } => {
                    if !appearance.valid() || result.object_guise.as_ref().is_some_and(|old| old != appearance) {
                        return Err(crate::creatures::BuildError::InvalidDefinition);
                    }
                    result.object_guise = Some(appearance.clone());
                },
                PerceptionValue::AmbushFromGuise { enabled } => { result.ambush_from_guise |= enabled; ambush_off |= !enabled; },
            }
        }
        result.invisible &= !invisible_off;
        result.see_invisible &= !detection_off;
        result.ambush_from_guise &= !ambush_off;
        if guise_off { result.object_guise = None; }
        Ok(result)
    }
}

/// The actor ID is already a never-reused incarnation. A different reference
/// domain must never be correlated with it by an observer or by its AI memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorReferenceKind { Creature, Contact }
impl<'de> Deserialize<'de> for ActorReferenceKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "creature" => Ok(Self::Creature),
            "contact" => Ok(Self::Contact),
            _ => Err(serde::de::Error::custom("invalid actor reference kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorDisclosure { Hidden, AnonymousContact, ApparentObject, Creature }
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
    Reserved { origin: crate::IntentionId },
    Spent,
}
impl<'de> Deserialize<'de> for AmbushState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields { Unacquired {}, Available {}, Reserved { origin: crate::IntentionId }, Spent {} }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Unacquired {} => Self::Unacquired,
            Fields::Available {} => Self::Available,
            Fields::Reserved { origin } if origin.0 > 0 && origin.0 < u64::MAX => Self::Reserved { origin },
            Fields::Reserved { .. } => return Err(serde::de::Error::custom("invalid ambush owner")),
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
        let value = Self { entered: fields.entered, appearance: fields.appearance, ambush: fields.ambush };
        if (!value.entered && (value.appearance.is_some() || value.ambush != AmbushState::Unacquired))
            || (value.appearance.is_none() && value.ambush == AmbushState::Available) {
            return Err(serde::de::Error::custom("inconsistent guise state"));
        }
        Ok(value)
    }
}
impl GuiseState {
    pub(crate) fn appearance(&self) -> Option<&GuiseAppearance> { self.appearance.as_ref() }
    pub(crate) fn ambush_available(&self) -> bool { self.appearance.is_some() && self.ambush == AmbushState::Available }
    pub(crate) fn reveal(&mut self) {
        if !self.entered { return; }
        self.appearance = None;
        if !matches!(self.ambush, AmbushState::Reserved { .. }) { self.ambush = AmbushState::Spent; }
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
        if self.entered && (capabilities.object_guise.is_none() || moved) { self.reveal(); }
        if self.appearance.is_some() {
            match (&self.ambush, capabilities.ambush_from_guise) {
                (AmbushState::Unacquired, true) => self.ambush = AmbushState::Available,
                (AmbushState::Available, false) => self.ambush = AmbushState::Spent,
                _ => {},
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
    pub eye_visible: bool,
}
pub(crate) type PerceptionProjection = BTreeMap<ActorId, PerceivedActor>;

impl crate::Actor {
    pub(crate) fn perception(&self) -> PerceptionCapabilities {
        self.combat.as_ref().map_or_else(PerceptionCapabilities::default, |combat| combat.creature().derived().perception.clone())
    }
}
impl Game {
    pub fn actor_perception(&self, id: ActorId) -> Option<PerceptionCapabilities> { self.actors.get(&id).map(crate::Actor::perception) }

    /// Exact visual eligibility, independent of tactile/stair disclosure. A seen
    /// object is not a license to target the creature concealed by its guise.
    pub(crate) fn actor_visible_in_sight(&self, observer: ActorId, subject: ActorId, sight: &[SightCell]) -> bool {
        let Some(owner) = self.actors.get(&observer) else { return false; };
        let Some(target) = self.actors.get(&subject) else { return false; };
        let capabilities = owner.perception();
        if !capabilities.sight || !target.alive() || target.guise.appearance().is_some()
            || (target.perception().invisible && !capabilities.see_invisible) { return false; }
        let Some(body) = self.body_cells(target.location, target.orientation, &target.body) else { return false; };
        let cells: BTreeSet<_> = body.into_iter().map(|(at, _)| at).collect();
        sight.iter().any(|cell| cells.contains(&cell.location))
    }
    pub(crate) fn genuinely_sees_actor_in_sight(&self, observer: ActorId, subject: ActorId, range: u16, sight: &[SightCell]) -> bool {
        if !self.actor_visible_in_sight(observer, subject, sight) { return false; }
        let target = &self.actors[&subject];
        let Some(body) = self.body_cells(target.location, target.orientation, &target.body) else { return false; };
        let cells: BTreeSet<_> = body.into_iter().map(|(at, _)| at).collect();
        sight.iter().any(|cell| cells.contains(&cell.location) && cell.offset.x.unsigned_abs() + cell.offset.y.unsigned_abs() + cell.offset.z.unsigned_abs() <= u32::from(range))
    }
    pub(crate) fn genuinely_sees_actor(&self, observer: ActorId, subject: ActorId, range: u16) -> bool {
        self.scene_snapshot(observer).is_ok_and(|(_, sight)| self.genuinely_sees_actor_in_sight(observer, subject, range, &sight))
    }
    pub(crate) fn perceived_hostile(&self, observer: ActorId, subject: ActorId, disclosure: ActorDisclosure) -> bool {
        let Some(reference) = disclosure.reference() else { return false; };
        observer != subject && ((reference == ActorReferenceKind::Creature && self.faction_hostile(observer, subject))
            || self.actors.get(&observer).and_then(|actor| actor.engagement_at(self.actor_clock(observer)))
                .is_some_and(|engagement| engagement.target == subject && engagement.reference == reference))
    }

    pub(crate) fn classified_actors(&self, observer: ActorId) -> Result<PerceptionProjection, GameError> {
        let (scene, sight) = self.scene_snapshot(observer)?;
        self.perception_projection(observer, &scene, &sight)
    }

    pub(crate) fn perception_projection(&self, observer: ActorId, scene: &[SightCell], sight: &[SightCell]) -> Result<PerceptionProjection, GameError> {
        self.perception_projection_at(observer, scene, sight, None)
    }
    pub(crate) fn perception_projection_at(&self, observer: ActorId, scene: &[SightCell], sight: &[SightCell], placement: Option<crate::abilities::Placement>) -> Result<PerceptionProjection, GameError> {
        let owner = self.actors.get(&observer).ok_or(GameError::UnknownActor)?;
        let capabilities = owner.perception();
        let cells: BTreeSet<_> = scene.iter().map(|cell| cell.location).collect();
        let eyes: BTreeSet<_> = sight.iter().map(|cell| cell.location).collect();
        let awareness = placement.map_or_else(|| self.local_awareness(observer), |at| self.local_awareness_at(at.location, at.orientation, &owner.body));
        let contact: BTreeSet<_> = awareness.into_iter().filter(|cell| cell.physical).map(|cell| cell.location).collect();
        Ok(self.actors.perceived(&self.world, &cells).into_iter().map(|(subject, body)| {
            let state = &self.actors[&subject];
            let physical_contact = body.iter().any(|(at, _)| contact.contains(at)) && self.melee_reachable_at_placement(observer, subject, scene, placement);
            let eye_visible = capabilities.sight && body.iter().any(|(at, _)| eyes.contains(at));
            let disclosure = if subject == observer { ActorDisclosure::Creature }
                else if state.perception().invisible && !(capabilities.see_invisible && eye_visible) {
                    if physical_contact { ActorDisclosure::AnonymousContact } else { ActorDisclosure::Hidden }
                } else if state.guise.appearance().is_some() { ActorDisclosure::ApparentObject }
                else { ActorDisclosure::Creature };
            (subject, PerceivedActor { disclosure, body, eye_visible })
        }).collect())
    }
}

#[cfg(test)]
mod tests;
