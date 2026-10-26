//! Strict offline scene descriptions. No runtime or filesystem effects in validation.
use crate::creatures::{Equipment, Look, Move, Variant, Wardrobe};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tor_client_3d_model::CreatureModel;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Script {
    pub version: u32,
    pub hz: u32,
    pub viewport: [u32; 2],
    pub actors: Vec<Actor>,
    pub cameras: Vec<CameraSpec>,
    pub events: Vec<Event>,
    #[serde(default)]
    pub captures: Vec<Capture>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Actor {
    pub id: String,
    pub model: String,
    pub seed: u64,
    pub body: Variant,
    pub outfit: String,
    pub equipment: Vec<String>,
    pub position: [f32; 3],
    pub yaw: f32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CameraSpec {
    pub id: String,
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub fov_degrees: f32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Capture {
    pub tick: u32,
    pub name: String,
    pub camera: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Event {
    pub tick: u32,
    pub actor: String,
    pub operation: Operation,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Operation {
    Action { name: String },
    Velocity { value: [f32; 3] },
    Teleport { position: [f32; 3], yaw: f32 },
    Dress { outfit: String },
    Equipment { items: Vec<String> },
}

pub(super) fn model(name: &str) -> Result<CreatureModel, String> {
    use CreatureModel::*;
    // Exact recipe identities, never the showcase's fallback lookup.
    let models = [
        Placeholder,
        Humanoid,
        Human,
        Sentinel,
        RedSentinel,
        BlueSentinel,
        Mage,
        RedMage,
        Scout,
        Guardian,
        Hobgoblin,
        Zombie,
        Vampire,
        Yeti,
        Troll,
        Bird,
        Emu,
        Equine,
        Quagga,
        Unicorn,
        BlackUnicorn,
        Centaur,
        Serpent,
        Snake,
        Wisp,
        Ice,
        Orc,
        Leprechaun,
        Nymph,
        Medusa,
        Phantom,
        Wraith,
        Bat,
        Kestrel,
        Griffin,
        Dragon,
        Jabberwock,
        Aquator,
        Xeroc,
        Flytrap,
        Rattlesnake,
    ];
    models
        .into_iter()
        .find(|m| format!("{m:?}") == name)
        .ok_or_else(|| format!("unknown model {name}"))
}
pub(super) fn equipment(items: &[String]) -> Result<Equipment, String> {
    let mut result = Equipment::default();
    let mut seen = HashSet::new();
    for item in items {
        if !seen.insert(item) {
            return Err(format!("duplicate equipment {item}"));
        }
        result.select(item)?;
    }
    Ok(result)
}
pub(super) fn action(name: &str) -> Result<Option<Move>, String> {
    Ok(match name {
        "cancel" => None,
        "jump" => Some(Move::Jump),
        "strike" => Some(Move::Strike),
        "thrust" => Some(Move::Thrust),
        "heavy" => Some(Move::Heavy),
        "focus" => Some(Move::Focus),
        "gesture" => Some(Move::Gesture),
        _ => return Err(format!("unsupported action {name}")),
    })
}
impl Actor {
    pub fn look(&self) -> Result<Look, String> {
        let base = Look::preview_adjusted(model(&self.model)?, self.seed, |v| *v = self.body);
        let wardrobe = Wardrobe::parse(&self.outfit)
            .ok_or_else(|| format!("unknown outfit {}", self.outfit))?;
        let gear = equipment(&self.equipment)?;
        base.composed(wardrobe, gear)
            .map_err(|error| format!("{}: {error}", self.id))
    }
}
fn safe_name(s: &str) -> bool {
    let upper = s.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
    {
        return false;
    }
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn vector(v: [f32; 3]) -> bool {
    v.into_iter().all(|x| x.is_finite() && x.abs() <= 1000.0)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Timeline {
    Capture,
    Live,
}
impl Script {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        Self::parse_for(bytes, Timeline::Capture)
    }
    pub fn parse_live(bytes: &[u8]) -> Result<Self, String> {
        Self::parse_for(bytes, Timeline::Live)
    }
    fn parse_for(bytes: &[u8], timeline: Timeline) -> Result<Self, String> {
        if bytes.len() > 1_048_576 {
            return Err("scene script exceeds 1 MiB".into());
        }
        let result: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        result.validate(timeline)?;
        Ok(result)
    }
    fn validate(&self, timeline: Timeline) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported scene script version".into());
        }
        crate::review_clock::ReviewStep::new(self.hz)?;
        if self.viewport.iter().any(|v| !(320..=2048).contains(v)) {
            return Err("viewport must be 320..2048".into());
        }
        if self.actors.is_empty()
            || self.actors.len() > 32
            || self.cameras.is_empty()
            || self.cameras.len() > 16
            || self.events.len() > 4096
            || (timeline == Timeline::Capture && self.captures.is_empty())
            || self.captures.len() > 256
        {
            return Err("scene record count outside limits".into());
        }
        let mut actors = HashSet::new();
        for actor in &self.actors {
            if !safe_name(&actor.id) || !actors.insert(&actor.id) {
                return Err("invalid or duplicate actor id".into());
            }
            if !vector(actor.position) || !actor.yaw.is_finite() || actor.yaw.abs() > 360.0 {
                return Err("invalid actor placement".into());
            }
            let b = actor.body;
            if b.build > 2
                || b.bust > 4
                || b.bust_form > 8
                || b.bust_lean > 2
                || b.figure > 2
                || b.skin > 4
                || b.hair > 4
                || b.hair_tone > 5
                || b.shade > 4
                || b.pattern > 3
            {
                return Err("body variation outside supported bounds".into());
            }
            actor.look()?;
        }
        let mut cameras = HashSet::new();
        for c in &self.cameras {
            let direction = Vec3::from_array(c.target) - Vec3::from_array(c.position);
            if !safe_name(&c.id)
                || !cameras.insert(&c.id)
                || !vector(c.position)
                || !vector(c.target)
                || direction.length() < 0.01
                || direction.cross(Vec3::Y).length() < 0.01
                || !c.fov_degrees.is_finite()
                || !(10.0..=120.0).contains(&c.fov_degrees)
            {
                return Err("invalid camera".into());
            }
        }
        let end = match timeline {
            Timeline::Capture => {
                let mut names = HashSet::new();
                for c in &self.captures {
                    if !safe_name(&c.name)
                        // Capture IDs become filenames: enforce portable uniqueness while
                        // retaining the authored spelling in manifests and output paths.
                        || !names.insert(c.name.to_ascii_lowercase())
                        || !cameras.contains(&c.camera)
                        || c.tick > self.hz * 120
                    {
                        return Err("invalid capture name, camera or tick".into());
                    }
                }
                self.captures.iter().map(|c| c.tick).max().unwrap()
            }
            Timeline::Live => self.hz * 600,
        };
        let mut resolved = self.actors.clone();
        let mut previous = 0;
        for event in &self.events {
            if event.tick < previous || event.tick > end {
                return Err(match timeline {
                    Timeline::Capture => "events must be ordered and precede final capture",
                    Timeline::Live => "live events must be ordered and within 600 seconds",
                }
                .into());
            }
            previous = event.tick;
            let a = resolved
                .iter_mut()
                .find(|a| a.id == event.actor)
                .ok_or("unknown event actor")?;
            match &event.operation {
                Operation::Action { name } => {
                    action(name)?;
                    if event.tick == 0 {
                        return Err("actions start at tick 1 or later".into());
                    }
                }
                Operation::Velocity { value } => {
                    if !vector(*value) || Vec3::from_array(*value).length() / self.hz as f32 > 0.25
                    {
                        return Err("invalid velocity".into());
                    }
                }
                Operation::Teleport { position, yaw } => {
                    if !vector(*position) || !yaw.is_finite() || yaw.abs() > 360.0 {
                        return Err("invalid teleport".into());
                    }
                }
                Operation::Dress { outfit } => {
                    a.outfit = outfit.clone();
                    a.look()?;
                }
                Operation::Equipment { items } => {
                    a.equipment = items.clone();
                    a.look()?;
                }
            }
        }
        if timeline == Timeline::Capture && self.captures.windows(2).any(|w| w[0].tick > w[1].tick)
        {
            return Err("captures must be ordered by tick".into());
        }
        Ok(())
    }
}
