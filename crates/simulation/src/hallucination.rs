//! Observer-only presentation after genuine acquisition. No identity, navigation,
//! targeting, condition lifetime or rules RNG authority is derived from samples.
use crate::appearance_catalog::{CreaturePresentation, ObjectPresentation};
use crate::appearance_state::{AppearanceChannel, AppearanceKey};
use crate::{ActorId, Game, GameError, Observation};
use std::collections::BTreeSet;
use tor_world::{Direction, Location, SightCell};

fn terrain_subject(at: Location) -> u64 {
    // Private deterministic location domain, including region and signed axes.
    [
        at.region.0,
        at.position.x as u32 as u64,
        at.position.y as u32 as u64,
        at.position.z as u32 as u64,
    ]
    .into_iter()
    .fold(0x7465727261696e31, |hash, word| {
        (hash ^ word).wrapping_mul(0x100000001b3)
    })
}
impl Game {
    fn presentation_key(
        &self,
        observer: ActorId,
        subject: u64,
        channel: AppearanceChannel,
        phase: u64,
    ) -> AppearanceKey {
        AppearanceKey {
            seed: self.seed,
            observer: observer.0,
            subject,
            channel,
            phase,
        }
    }
    fn cosmetic_object(
        &self,
        observer: ActorId,
        subject: u64,
        channel: AppearanceChannel,
        phase: u64,
    ) -> ObjectPresentation {
        self.content
            .presentation_catalog()
            .object(self.presentation_key(observer, subject, channel, phase))
    }
    pub(crate) fn hallucinated_actor_presentation(
        &self,
        observer: ActorId,
        subject: ActorId,
        sight: &[SightCell],
    ) -> Result<Option<CreaturePresentation>, GameError> {
        if observer == subject || !self.actor_visible_in_sight(observer, subject, sight) {
            return Ok(None);
        }
        let Some(clock) = self.projected_hallucination(observer)? else {
            return Ok(None);
        };
        Ok(Some(self.content.presentation_catalog().creature(
            self.presentation_key(
                observer,
                subject.0,
                AppearanceChannel::Creature,
                clock.phase(),
            ),
        )))
    }
    fn genuinely_sees_guise(
        &self,
        observer: ActorId,
        subject: ActorId,
        eyes: &BTreeSet<Location>,
    ) -> bool {
        let owner = &self.actors[&observer];
        let target = &self.actors[&subject];
        let senses = owner.perception();
        senses.sight
            && target.alive()
            && (!target.perception().invisible || senses.see_invisible)
            && self
                .body_cells(target.location, target.orientation, &target.body)
                .is_some_and(|body| body.iter().any(|(at, _)| eyes.contains(at)))
    }
    fn genuinely_known_stair(&self, observer: ActorId, at: Location) -> bool {
        self.explored_maps
            .get(&observer)
            .and_then(|map| map.cells.get(&at))
            .is_some_and(|appearance| {
                (appearance.stairs_up || appearance.stairs_down)
                    && appearance.object_presentation.is_none()
            })
    }
    pub(crate) fn apply_hallucinated_presentation(
        &self,
        view: &mut Observation,
        sight: &[SightCell],
    ) -> Result<(), GameError> {
        let observer = view.actor;
        let Some(clock) = self.projected_hallucination(observer)? else {
            return Ok(());
        };
        let phase = clock.phase();
        let eyes: BTreeSet<_> = sight.iter().map(|cell| cell.location).collect();
        for item in &mut view.ground_items {
            if !eyes.contains(&item.location) {
                continue;
            }
            item.presentation =
                Some(self.cosmetic_object(observer, item.id.0, AppearanceChannel::Item, phase));
            item.name = "object".into();
            item.appearance = "object".into();
            item.description.clear();
            item.asset = None;
            item.corpse = None;
        }
        for object in &mut view.apparent_objects {
            if !self.genuinely_sees_guise(observer, object.subject, &eyes) {
                continue;
            }
            object.presentation = Some(self.cosmetic_object(
                observer,
                object.subject.0,
                AppearanceChannel::Item,
                phase,
            ));
            object.appearance.name = "object".into();
            object.appearance.description.clear();
            object.appearance.asset = None;
        }
        for actor in &mut view.visible_actors {
            if let Some(presentation) =
                self.hallucinated_actor_presentation(observer, actor.id, sight)?
            {
                actor.name = presentation.name.to_string();
                actor.asset = presentation.asset.as_ref().map(ToString::to_string);
                // A hallucinated figure shows no true gear: that would betray it.
                actor.worn.clear();
                actor.presentation = Some(presentation);
            }
        }
        let disclosed_stairs: BTreeSet<_> = view
            .exits
            .iter()
            .filter(|exit| matches!(exit.direction, Direction::Up | Direction::Down))
            .map(|exit| exit.location)
            .collect();
        for cell in &mut view.visible_cells {
            if eyes.contains(&cell.location)
                && !self.genuinely_known_stair(observer, cell.location)
                && !cell.wall
                && disclosed_stairs.contains(&cell.location)
            {
                cell.object_presentation = Some(self.cosmetic_object(
                    observer,
                    terrain_subject(cell.location),
                    AppearanceChannel::Terrain,
                    phase,
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "hallucination_view_tests.rs"]
pub(crate) mod tests;
