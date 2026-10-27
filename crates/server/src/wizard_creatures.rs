//! Host-only named construction from pinned, already compiled scenario recipes.
//! No species defaults, runtime compiler, or source-file lookup lives here.
use crate::{regions::Regions, Failure};
use std::collections::{BTreeMap, BTreeSet};
use tor_protocol::{CreatureCatalogEntry, CreatureCatalogPage, ErrorCode};
use tor_simulation::{floor_activation::PreparedActorChoice, Game};

pub(crate) fn valid_selector(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 768
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn encode(value: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.') {
            out.push(char::from(byte));
        } else {
            write!(out, "%{byte:02X}").expect("String write");
        }
    }
    out
}
fn decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = bytes.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(out).ok()?;
    (!decoded.is_empty() && decoded.len() <= 80 && encode(&decoded) == value).then_some(decoded)
}
fn population_selector(profile: &str, stage: &str, archetype: &str) -> String {
    format!(
        "population:{}/{}/{}",
        encode(profile),
        encode(stage),
        encode(archetype)
    )
}
fn unavailable() -> Failure {
    Failure::new(ErrorCode::InvalidAction,
        "Creature recipe is unavailable or ambiguous; use creature list, an explicit depth, or a qualified selector")
}

pub(crate) enum ResolvedCreature {
    Population {
        profile: String,
        stage: String,
        archetype: String,
        selector: String,
    },
    Standalone {
        selector: String,
        definition: Box<PreparedActorChoice>,
    },
}

pub(crate) fn resolve(
    game: &Game,
    regions: &Regions,
    selector: &str,
    depth: Option<u32>,
) -> Result<ResolvedCreature, Failure> {
    if !valid_selector(selector) || depth == Some(0) {
        return Err(unavailable());
    }
    let standalone = |id: &str| -> Result<ResolvedCreature, Failure> {
        Ok(ResolvedCreature::Standalone {
            selector: format!("archetype:{}", encode(id)),
            definition: Box::new(regions.standalone_creature(id).map_err(|_| unavailable())?),
        })
    };
    if let Some(id) = selector.strip_prefix("archetype:") {
        return standalone(&decode(id).ok_or_else(unavailable)?);
    }
    if let Some(path) = selector.strip_prefix("population:") {
        let parts: Vec<_> = path.split('/').collect();
        let [profile, stage, archetype] = parts.as_slice() else {
            return Err(unavailable());
        };
        let (profile, stage, archetype) = (
            decode(profile).ok_or_else(unavailable)?,
            decode(stage).ok_or_else(unavailable)?,
            decode(archetype).ok_or_else(unavailable)?,
        );
        game.population_spawn_plan()
            .ok_or_else(unavailable)?
            .named_actor(&profile, &stage, &archetype, depth)
            .map_err(|_| unavailable())?;
        return Ok(ResolvedCreature::Population {
            profile,
            stage,
            archetype,
            selector: selector.into(),
        });
    }
    let archetype = decode(selector).ok_or_else(unavailable)?;
    let mut found = None;
    let mut declared = false;
    if let Some(plan) = game.population_spawn_plan() {
        for (profile_id, profile) in &plan.profiles {
            for stage in &profile.stages {
                let Some(bands) = stage.population.actor_bands() else {
                    continue;
                };
                if !bands
                    .iter()
                    .flat_map(|band| &band.choices)
                    .any(|choice| choice.content == archetype)
                {
                    continue;
                }
                declared = true;
                match plan.named_actor(profile_id, &stage.id, &archetype, depth) {
                    Ok(_) => {
                        if found.is_some() {
                            return Err(unavailable());
                        }
                        found = Some(ResolvedCreature::Population {
                            profile: profile_id.clone(),
                            stage: stage.id.clone(),
                            archetype: archetype.clone(),
                            selector: population_selector(profile_id, &stage.id, &archetype),
                        });
                    }
                    Err(_) if depth.is_none() => return Err(unavailable()),
                    Err(_) => {}
                }
            }
        }
    }
    match found {
        Some(found) => Ok(found),
        None if !declared => standalone(&archetype),
        None => Err(unavailable()),
    }
}

fn entry(
    selector: String,
    definition: &PreparedActorChoice,
    standalone: bool,
    depth: Option<u32>,
) -> CreatureCatalogEntry {
    let name = definition
        .actor
        .identity
        .as_ref()
        .expect("creature recipe")
        .name
        .clone();
    let details = if standalone {
        "Authored base; external control; no population AI or birth stock".into()
    } else {
        let control = if definition.actor.ai.is_some() {
            "authored AI"
        } else {
            "external control"
        };
        let stock = definition.carried_loot.as_ref().map_or_else(
            || "no birth stock".into(),
            |policy| {
                format!(
                    "stock pool {} ({}/{})",
                    policy.pool, policy.chance.numerator, policy.chance.denominator
                )
            },
        );
        let growth = match (depth, definition.depth_growth) {
            (None, _) => "authored base".into(),
            (Some(depth), tor_simulation::prepared_spawn::DepthGrowth::Fixed {}) => {
                format!("depth {depth}; fixed build")
            }
            (
                Some(depth),
                tor_simulation::prepared_spawn::DepthGrowth::RacialAfter { depth: after },
            ) => format!("depth {depth}; racial growth after {after}"),
        };
        format!("{growth}; {control}; {stock}")
    };
    CreatureCatalogEntry {
        selector,
        name,
        details,
    }
}

pub(crate) fn catalog(
    game: &Game,
    regions: Option<&Regions>,
    depth: Option<u32>,
    after: Option<&str>,
) -> Result<CreatureCatalogPage, Failure> {
    if depth == Some(0) || after.is_some_and(|after| !valid_selector(after)) {
        return Err(unavailable());
    }
    // Retain only one response page and lookahead, regardless of catalog size.
    let mut rows = BTreeMap::new();
    let mut cursor_found = after.is_none();
    let mut add = |entry: CreatureCatalogEntry| {
        if after == Some(entry.selector.as_str()) {
            cursor_found = true;
        }
        if after.is_some_and(|after| entry.selector.as_str() <= after) {
            return;
        }
        rows.insert(entry.selector.clone(), entry);
        if rows.len() > 33 {
            rows.pop_last();
        }
    };
    if let Some(regions) = regions {
        for (id, name) in regions.creature_archetypes() {
            add(CreatureCatalogEntry {
                selector: format!("archetype:{}", encode(id)),
                name: name.into(),
                details: "Authored base; external control; no population AI or birth stock".into(),
            });
        }
    }
    if let Some(plan) = game.population_spawn_plan() {
        for (profile_id, profile) in &plan.profiles {
            for stage in &profile.stages {
                let Some(bands) = stage.population.actor_bands() else {
                    continue;
                };
                let names: BTreeSet<_> = bands
                    .iter()
                    .flat_map(|band| &band.choices)
                    .filter(|choice| choice.actor.build.is_some())
                    .map(|choice| choice.content.as_str())
                    .collect();
                for name in names {
                    let selector = population_selector(profile_id, &stage.id, name);
                    match plan.named_actor(profile_id, &stage.id, name, depth) {
                        Ok((_, _, definition)) => add(entry(selector, definition, false, depth)),
                        Err(_) if depth.is_none() => add(CreatureCatalogEntry {
                            selector,
                            name: name.into(),
                            details: "Several authored depth recipes; explicit depth required"
                                .into(),
                        }),
                        Err(_) => {}
                    }
                }
            }
        }
    }
    if !cursor_found {
        return Err(unavailable());
    }
    let more = rows.len() > 32;
    if more {
        rows.pop_last();
    }
    let entries: Vec<_> = rows.into_values().collect();
    let next = more.then(|| entries.last().expect("full page").selector.clone());
    let page = CreatureCatalogPage {
        depth,
        entries,
        next,
    };
    page.validate()
        .map_err(|_| Failure::new(ErrorCode::InvalidArchive, "Invalid creature catalog"))?;
    Ok(page)
}

/// Bound the complete correlated wire envelope, including escaped cursor,
/// request and context fields. The catalog has already retained at most 32
/// metadata rows; trimming never copies or instantiates creature builds.
pub(crate) fn fit_reply(
    mut page: CreatureCatalogPage,
    context: &tor_protocol::ReplyContext,
    request_id: &str,
    limit: usize,
) -> Result<CreatureCatalogPage, Failure> {
    #[derive(serde::Serialize)]
    struct Envelope<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        context: &'a tor_protocol::ReplyContext,
        request_id: &'a str,
        page: &'a CreatureCatalogPage,
    }
    let invalid = || Failure::new(ErrorCode::InvalidArchive, "Invalid creature catalog");
    page.validate().map_err(|_| invalid())?;
    loop {
        let fits = tor_protocol::encoded_length(
            &Envelope {
                kind: "creature_catalog",
                context,
                request_id,
                page: &page,
            },
            limit.min(tor_protocol::MAX_RESPONSE_BYTES),
        )
        .map_err(|_| invalid())?
        .is_some();
        if fits {
            return Ok(page);
        }
        if page.entries.len() <= 1 {
            return Err(Failure::new(
                ErrorCode::ResourceLimit,
                "Creature catalog row exceeds response limit",
            ));
        }
        page.entries.pop();
        page.next = page.entries.last().map(|entry| entry.selector.clone());
    }
}

/// Checkpoint recovery validates the same pinned birth definition and independent
/// deterministic stock decision even after the original branch expires.
pub(crate) fn validate_birth_record(
    game: &Game,
    regions: Option<&Regions>,
    record: &crate::engine::Record,
) -> Result<(), Failure> {
    use crate::journal::{JournalContent, WizardOperation, WizardResult};
    let JournalContent::Wizard {
        operation, result, ..
    } = &record.entry.content
    else {
        return Ok(());
    };
    let (
        WizardOperation::SpawnCreature {
            selector,
            position,
            depth,
        },
        WizardResult::CreatureSpawned {
            actor,
            selector: resolved,
            depth: effective,
            carried_item,
        },
    ) = (operation, result)
    else {
        return if matches!(operation, WizardOperation::SpawnCreature { .. })
            || matches!(result, WizardResult::CreatureSpawned { .. })
        {
            Err(unavailable())
        } else {
            Ok(())
        };
    };
    let regions = regions.ok_or_else(unavailable)?;
    let (actor_floor, item_floor) = regions.named_birth_floors().ok_or_else(unavailable)?;
    if actor.0 < actor_floor
        || carried_item.is_some_and(|item| item < item_floor)
        || !record.named_spawn_receipt_matches(operation)
    {
        return Err(unavailable());
    }
    let depth = depth.or_else(|| regions.authored_depth(position.region));
    match resolve(game, regions, selector, depth)? {
        ResolvedCreature::Population {
            profile,
            stage,
            archetype,
            selector,
        } => {
            if &selector != resolved || &depth != effective {
                return Err(unavailable());
            }
            game.validate_named_population_birth(
                &profile,
                &stage,
                &archetype,
                depth,
                tor_simulation::ActorId(actor.0),
                carried_item.map(tor_simulation::ItemId),
            )
            .map_err(|_| unavailable())
        }
        ResolvedCreature::Standalone {
            selector,
            definition,
        } => {
            if &selector != resolved || effective.is_some() || carried_item.is_some() {
                return Err(unavailable());
            }
            game.validate_standalone_creature_birth(
                &definition.actor,
                tor_simulation::ActorId(actor.0),
                crate::creature_authoring::actor_health_seed,
            )
            .map_err(|_| unavailable())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creature_catalog_byte_budget_matches_actual_envelope_and_rejects_one_large_row() {
        let context: tor_protocol::ReplyContext = serde_json::from_value(serde_json::json!({
            "input":{"stream":{"stream":"stream\\\"escaped","epoch":"18446744073709551615"},
                "readiness_revision":"18446744073709551615"},
            "actor":"18446744073709551615","branch":"branch\\\"escaped",
            "cursor":{"sequence":"18446744073709551615","tick":"18446744073709551615"},
            "revision":"18446744073709551615"
        }))
        .unwrap();
        let request = "\\\"".repeat(64);
        let one = CreatureCatalogPage {
            depth: Some(u32::MAX),
            entries: vec![CreatureCatalogEntry {
                selector: "archetype:escaped%22".into(),
                name: "\"".repeat(80),
                details: "\\".repeat(400),
            }],
            next: Some("archetype:escaped%22".into()),
        };
        let actual = tor_protocol::ServerMessage::CreatureCatalog {
            context: context.clone(),
            request_id: request.clone(),
            page: Box::new(one.clone()),
        };
        let exact = serde_json::to_vec(&actual).unwrap().len();
        assert_eq!(
            fit_reply(one.clone(), &context, &request, exact).unwrap(),
            one
        );
        let failure = fit_reply(one.clone(), &context, &request, exact - 1).unwrap_err();
        assert_eq!(failure.code, ErrorCode::ResourceLimit);
        let error = tor_protocol::ServerMessage::Error {
            scope: tor_protocol::ErrorScope::Attached {
                context: context.clone(),
            },
            request_id: Some(request.clone()),
            code: failure.code,
            message: failure.message,
        };
        assert!(
            tor_protocol::encoded_length(&error, exact - 1)
                .unwrap()
                .is_some(),
            "the complete correlated rejection is itself bounded"
        );
        let mut two = one.clone();
        let mut later = two.entries[0].clone();
        later.selector = "archetype:later".into();
        two.entries.push(later);
        two.next = None;
        assert_eq!(fit_reply(two, &context, &request, exact).unwrap(), one);
        let empty = CreatureCatalogPage {
            depth: None,
            entries: vec![],
            next: None,
        };
        assert_eq!(
            fit_reply(empty, &context, &request, 1).unwrap_err().code,
            ErrorCode::ResourceLimit
        );
    }

    #[test]
    fn selector_components_preserve_authored_punctuation_and_unicode_without_aliases() {
        for id in ["rat", "a/b", "a:b", "a b", "é", "%"] {
            let escaped = encode(id);
            assert_eq!(decode(&escaped).as_deref(), Some(id));
            assert!(valid_selector(&escaped));
        }
        for invalid in ["", "%", "%FF", "%2f", "%41", "a b", "a/b"] {
            assert!(decode(invalid).is_none(), "{invalid}");
        }
        assert_ne!(
            population_selector("a/b", "c", "d"),
            population_selector("a", "b/c", "d")
        );
    }
}
