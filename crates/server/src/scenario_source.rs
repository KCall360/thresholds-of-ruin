//! Bounded author-source composition. Modules are data, never executable imports.
use super::*;

const MAX_MODULES: usize = 64;
const MAX_SOURCE_BYTES: u64 = 32 * 1024 * 1024;

/// Resolved authoring input, with raw integrity identities separate from semantics.
#[derive(Clone, Debug, Serialize)]
pub struct ResolvedSource {
    pub manifest: Manifest,
    pub manifest_text: String,
    pub source_hashes: BTreeMap<String, String>,
    /// Qualified declaration path to the source that owns it.
    pub provenance: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogModule {
    namespace: String,
    #[serde(default)]
    creatures: crate::creature_authoring::Catalog,
    #[serde(default)]
    archetypes: BTreeMap<String, Archetype>,
    #[serde(default)]
    ai_profiles: BTreeMap<String, AiProfile>,
    #[serde(default)]
    appearance_pools: BTreeMap<String, AppearancePool>,
    #[serde(default)]
    generation_recipes: BTreeMap<String, crate::generation_recipe::Recipe>,
    #[serde(default)]
    generation_groups: BTreeMap<String, crate::generation_recipe::GenerationGroup>,
    #[serde(default)]
    stair_pairs: BTreeMap<String, StairPair>,
    #[serde(default)]
    procedural_worlds: BTreeMap<String, crate::procedural_world::WorldFamily>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActorDefinition {
    creature: Option<crate::creature_authoring::BuildSpec>,
    anatomy: Option<AnatomySpec>,
    body: Option<BodySpec>,
    turn_ticks: Option<u64>,
    asset: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemDefinition {
    name: Option<String>,
    #[serde(default)]
    class: ItemClass,
    equipment: Option<EquipmentSpec>,
    consumable: Option<ConsumableSpec>,
    readable: Option<ReadableSpec>,
    device: Option<DeviceSpec>,
    edible: Option<EdibleSpec>,
    supply: Option<String>,
    #[serde(default)]
    magical: bool,
    currency: Option<CurrencyKind>,
    identity: Option<String>,
    appearance_pool: Option<String>,
    #[serde(default)]
    stackable: bool,
    #[serde(default)]
    properties: BTreeMap<String, String>,
    asset: Option<String>,
}

fn normalize_catalog_views(
    value: &mut toml::Value,
    file: &str,
) -> Result<(bool, BTreeMap<String, &'static str>), Failure> {
    let table = value
        .as_table_mut()
        .ok_or_else(|| fail(format!("{file}: expected document table")))?;
    let mut origins = BTreeMap::new();
    let mut changed = false;
    for field in ["actors", "items"] {
        let Some(entries) = table.remove(field) else {
            continue;
        };
        changed = true;
        let entries = entries
            .as_table()
            .ok_or_else(|| fail(format!("{file}: {field}: expected catalog table")))?;
        for (id, entry) in entries {
            let archetype = if field == "actors" {
                let actor: ActorDefinition = entry
                    .clone()
                    .try_into()
                    .map_err(|e| fail(format!("{file}: actors.{id}: {e}")))?;
                Archetype {
                    creature: actor.creature,
                    anatomy: actor.anatomy,
                    body: actor.body,
                    turn_ticks: actor.turn_ticks,
                    asset: actor.asset,
                    ..Default::default()
                }
            } else {
                let item: ItemDefinition = entry
                    .clone()
                    .try_into()
                    .map_err(|e| fail(format!("{file}: items.{id}: {e}")))?;
                Archetype {
                    name: item.name,
                    class: item.class,
                    equipment: item.equipment,
                    consumable: item.consumable,
                    readable: item.readable,
                    device: item.device,
                    edible: item.edible,
                    supply: item.supply,
                    magical: item.magical,
                    currency: item.currency,
                    identity: item.identity,
                    appearance_pool: item.appearance_pool,
                    stackable: item.stackable,
                    properties: item.properties,
                    asset: item.asset,
                    ..Default::default()
                }
            };
            let catalog = table
                .entry("archetypes")
                .or_insert_with(|| toml::Value::Table(Default::default()))
                .as_table_mut()
                .ok_or_else(|| fail(format!("{file}: archetypes must be a table")))?;
            require(
                !catalog.contains_key(id),
                format!("{file}: duplicate lowered archetype ID {id}"),
            )?;
            catalog.insert(
                id.clone(),
                toml::Value::try_from(archetype).map_err(|e| fail(e.to_string()))?,
            );
            origins.insert(id.clone(), field);
        }
    }
    Ok((changed, origins))
}

fn namespace(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-')
                })
        })
}

fn merge<T>(
    target: &mut BTreeMap<String, T>,
    values: BTreeMap<String, T>,
    field: &str,
    scope: &str,
    file: &str,
    provenance: &mut BTreeMap<String, String>,
) -> Result<(), Failure> {
    for (id, value) in values {
        require(
            namespace(&id) && id.starts_with(&format!("{scope}.")),
            format!("{file}: {field}.{id}: declaration must belong to namespace {scope}"),
        )?;
        require(
            !target.contains_key(&id),
            format!("{file}: duplicate declaration {field}.{id}"),
        )?;
        provenance.insert(format!("{field}.{id}"), file.into());
        target.insert(id, value);
    }
    Ok(())
}

fn module_path(root: &Path, file: &str) -> Result<(), Failure> {
    require(
        valid_rules_path(file)
            && file.len() <= 256
            && Path::new(file)
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
                == file,
        format!("{file}: module must name a package-local TOML file"),
    )?;
    let mut path = root.to_path_buf();
    for part in Path::new(file).components() {
        path.push(part);
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|e| fail(format!("{file}: {e}")))?;
        require(
            !metadata.file_type().is_symlink(),
            format!("{file}: module symlinks are not allowed"),
        )?;
    }
    require(
        path.is_file(),
        format!("{file}: module must be a regular file"),
    )
}

/// Lower the honest named algorithm preset, keeping runtime recipes internal.
fn normalize_recipes(value: &mut toml::Value, file: &str) -> Result<bool, Failure> {
    let Some(recipes) = value
        .get_mut("generation_recipes")
        .and_then(toml::Value::as_table_mut)
    else {
        return Ok(false);
    };
    let mut changed = false;
    for (id, recipe) in recipes {
        let Some(kind) = recipe.get("kind") else {
            continue;
        };
        require(
            kind.as_str() == Some("grid_rooms"),
            format!("{file}: generation_recipes.{id}: unknown generator preset"),
        )?;
        let mut preset = recipe.clone();
        preset
            .as_table_mut()
            .ok_or_else(|| fail("Expected generator preset table"))?
            .remove("kind");
        let preset: crate::generation_recipe::GridRoomsPreset = preset
            .try_into()
            .map_err(|e| fail(format!("{file}: generation_recipes.{id}: {e}")))?;
        let lowered = preset
            .to_recipe()
            .map_err(|e| in_declaration(e, format!("{file}: generation_recipes.{id}")))?;
        *recipe = toml::Value::try_from(lowered).map_err(|e| fail(e.to_string()))?;
        changed = true;
    }
    Ok(changed)
}

pub(super) fn declaration_file<'a>(
    provenance: &'a BTreeMap<String, String>,
    field: &str,
    id: &str,
) -> &'a str {
    provenance
        .get(&format!("{field}.{id}"))
        .map(String::as_str)
        .unwrap_or("scenario.toml")
}

pub(super) fn archetype_origin<'a>(
    provenance: &'a BTreeMap<String, String>,
    name: &'a str,
) -> Origin<'a> {
    let catalog = if provenance.contains_key(&format!("actors.{name}")) {
        "actors"
    } else if provenance.contains_key(&format!("items.{name}")) {
        "items"
    } else {
        "archetypes"
    };
    Origin::Archetype {
        file: declaration_file(provenance, "archetypes", name),
        name,
        catalog,
    }
}

pub(super) fn catalog_context(
    provenance: &BTreeMap<String, String>,
    kind: crate::creature_authoring::CatalogKind,
    id: &str,
    failure: Failure,
) -> Failure {
    let (field, label) = match kind {
        crate::creature_authoring::CatalogKind::Species => ("creatures.species", "species"),
        crate::creature_authoring::CatalogKind::Template => ("creatures.templates", "template"),
    };
    in_declaration(
        failure,
        format!(
            "{}: {label} {id:?}",
            declaration_file(provenance, field, id)
        ),
    )
}

/// Resolve only the declared source closure. No directory scans or network lookup.
pub fn resolve_source(root: &Path) -> Result<ResolvedSource, Failure> {
    let authored = read(root, "scenario.toml")?;
    let mut value: toml::Value = parse(&authored, "scenario.toml")?;
    let module_value = value
        .as_table_mut()
        .ok_or_else(|| fail("scenario.toml: expected table"))?
        .remove("modules");
    let has_modules = module_value.is_some();
    let normalized_recipes = normalize_recipes(&mut value, "scenario.toml")?;
    let (normalized_catalogs, root_catalogs) =
        normalize_catalog_views(&mut value, "scenario.toml")?;
    let modules: Vec<String> = match module_value {
        Some(value) => value
            .try_into()
            .map_err(|e| fail(format!("scenario.toml: modules: {e}")))?,
        None => Vec::new(),
    };
    require(
        modules.len() <= MAX_MODULES,
        "scenario.toml: at most 64 catalog modules are allowed",
    )?;
    let root_text = if !has_modules && !normalized_recipes && !normalized_catalogs {
        authored.clone()
    } else {
        toml::to_string(&value).map_err(|e| fail(e.to_string()))?
    };
    // Even an explicitly empty modules list must be removed from the pinned DTO.
    let mut manifest: Manifest = parse(&root_text, "scenario.toml")?;
    check_manifest_header(&manifest)?;
    let mut provenance: BTreeMap<String, String> = root_catalogs
        .into_iter()
        .map(|(id, field)| (format!("{field}.{id}"), "scenario.toml".into()))
        .collect();
    for (field, ids) in [
        (
            "creatures.species",
            manifest.creatures.species.keys().collect::<Vec<_>>(),
        ),
        (
            "creatures.templates",
            manifest.creatures.templates.keys().collect(),
        ),
        ("archetypes", manifest.archetypes.keys().collect()),
        (
            "generation_groups",
            manifest.generation_groups.keys().collect(),
        ),
        ("stair_pairs", manifest.stair_pairs.keys().collect()),
        (
            "procedural_worlds",
            manifest.procedural_worlds.keys().collect(),
        ),
        ("ai_profiles", manifest.ai_profiles.keys().collect()),
        (
            "appearance_pools",
            manifest.appearance_pools.keys().collect(),
        ),
        (
            "generation_recipes",
            manifest.generation_recipes.keys().collect(),
        ),
    ] {
        for id in ids {
            provenance.insert(format!("{field}.{id}"), "scenario.toml".into());
        }
    }
    let mut source_hashes = BTreeMap::from([("scenario.toml".into(), source_digest(&authored))]);
    let mut total = authored.len() as u64;
    for file in &modules {
        require(
            !source_hashes.contains_key(file),
            format!("{file}: duplicate module include"),
        )?;
        module_path(root, file)?;
        let text = read_limited(
            root,
            file,
            MAX_BYTES.min(MAX_SOURCE_BYTES.saturating_sub(total)),
        )?;
        total += text.len() as u64;
        require(
            total <= MAX_SOURCE_BYTES,
            "Catalog source closure exceeds 32 MiB",
        )?;
        let mut value: toml::Value = parse(&text, file)?;
        let lowered = normalize_recipes(&mut value, file)?;
        let (normalized_catalogs, module_catalogs) = normalize_catalog_views(&mut value, file)?;
        let module: CatalogModule = if lowered || normalized_catalogs {
            value.try_into().map_err(|e| fail(format!("{file}: {e}")))?
        } else {
            parse(&text, file)?
        };
        require(
            namespace(&module.namespace),
            format!("{file}: invalid module namespace"),
        )?;
        let scope = &module.namespace;
        merge(
            &mut manifest.creatures.species,
            module.creatures.species,
            "creatures.species",
            scope,
            file,
            &mut provenance,
        )?;
        merge(
            &mut manifest.creatures.templates,
            module.creatures.templates,
            "creatures.templates",
            scope,
            file,
            &mut provenance,
        )?;
        merge(
            &mut manifest.archetypes,
            module.archetypes,
            "archetypes",
            scope,
            file,
            &mut provenance,
        )?;
        merge(
            &mut manifest.ai_profiles,
            module.ai_profiles,
            "ai_profiles",
            scope,
            file,
            &mut provenance,
        )?;
        merge(
            &mut manifest.appearance_pools,
            module.appearance_pools,
            "appearance_pools",
            scope,
            file,
            &mut provenance,
        )?;
        merge(
            &mut manifest.generation_recipes,
            module.generation_recipes,
            "generation_recipes",
            scope,
            file,
            &mut provenance,
        )?;
        merge(
            &mut manifest.generation_groups,
            module.generation_groups,
            "generation_groups",
            scope,
            file,
            &mut provenance,
        )?;
        merge(
            &mut manifest.stair_pairs,
            module.stair_pairs,
            "stair_pairs",
            scope,
            file,
            &mut provenance,
        )?;
        merge(
            &mut manifest.procedural_worlds,
            module.procedural_worlds,
            "procedural_worlds",
            scope,
            file,
            &mut provenance,
        )?;
        for (id, field) in module_catalogs {
            provenance.insert(format!("{field}.{id}"), file.clone());
        }
        source_hashes.insert(file.clone(), source_digest(&text));
    }
    let manifest_text = if modules.is_empty() {
        root_text
    } else {
        toml::to_string(&manifest).map_err(|e| fail(e.to_string()))?
    };
    Ok(ResolvedSource {
        manifest,
        manifest_text,
        source_hashes,
        provenance,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_package(includes: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let source = include_str!("../../../scenarios/two-room/scenario.toml");
        std::fs::write(
            root.path().join("scenario.toml"),
            format!("modules = {includes}\n{source}"),
        )
        .unwrap();
        root
    }

    #[test]
    fn module_count_and_source_byte_limits_fail_before_outputs() {
        let names: Vec<_> = (0..64).map(|i| format!("module-{i}.toml")).collect();
        let root = source_package(&serde_json::to_string(&names).unwrap());
        for (i, name) in names.iter().enumerate() {
            std::fs::write(root.path().join(name), format!("namespace = \"part{i}\"\n")).unwrap();
        }
        assert_eq!(resolve_source(root.path()).unwrap().source_hashes.len(), 65);
        let mut over = names;
        over.push("never-read.toml".into());
        let too_many = source_package(&serde_json::to_string(&over).unwrap());
        assert!(resolve_source(too_many.path())
            .unwrap_err()
            .message
            .contains("at most 64"));
        let oversized = source_package("[\"large.toml\"]");
        std::fs::write(
            oversized.path().join("large.toml"),
            vec![b' '; MAX_BYTES as usize + 1],
        )
        .unwrap();
        assert!(resolve_source(oversized.path()).is_err());
        let files = ["a.toml", "b.toml", "c.toml", "d.toml"];
        let closure = source_package(&serde_json::to_string(&files).unwrap());
        for (i, file) in files.iter().enumerate() {
            let mut text = format!("namespace = \"large{i}\"\n#");
            text.extend(std::iter::repeat_n('x', MAX_BYTES as usize - text.len()));
            std::fs::write(closure.path().join(file), text).unwrap();
        }
        let error = resolve_source(closure.path()).unwrap_err();
        assert!(error.message.contains("d.toml"), "{}", error.message);
        for directory in [
            root.path(),
            too_many.path(),
            oversized.path(),
            closure.path(),
        ] {
            assert!(!directory.join("index.json").exists());
            assert!(!directory.join("validation.json").exists());
            assert!(!directory.join(".compiled").exists());
        }
    }

    fn checked_module(module: &str) -> (tempfile::TempDir, Package) {
        let root = source_package("[\"catalog.toml\"]");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        std::fs::copy(fixture.join("rules.toml"), root.path().join("rules.toml")).unwrap();
        std::fs::create_dir(root.path().join("regions")).unwrap();
        for file in std::fs::read_dir(fixture.join("regions")).unwrap() {
            let file = file.unwrap();
            std::fs::copy(
                file.path(),
                root.path().join("regions").join(file.file_name()),
            )
            .unwrap();
        }
        std::fs::write(root.path().join("catalog.toml"), module).unwrap();
        let package = read_package(root.path()).unwrap();
        (root, package)
    }

    #[test]
    fn module_stair_endpoint_spans_require_matching_original_source() {
        for (upper, lower, field, reference) in [
            ("1/missing", "2/landing", "upper", "1/missing"),
            ("1/start", "2/missing", "lower", "2/missing"),
        ] {
            let authored = format!(
                "namespace = \"demo\"\n[stair_pairs.\"demo.entry\"]\nupper = \"{upper}\"\nlower = \"{lower}\"\n"
            );
            let (root, package) = checked_module(&authored);
            let line = if field == "upper" { 3 } else { 4 };
            let message = package.index(0).unwrap_err().message;
            let prefix = format!("catalog.toml:{line}:9: stair pair \"demo.entry\": ");
            assert!(message.starts_with(&prefix), "{message}");
            assert!(message.contains(reference), "{message}");
            let semantic = message.strip_prefix(&prefix).unwrap();
            let fallback = format!("catalog.toml: stair pair \"demo.entry\": {semantic}");
            let path = root.path().join("catalog.toml");
            std::fs::write(&path, format!("# changed after resolution\n{authored}")).unwrap();
            assert_eq!(package.index(0).unwrap_err().message, fallback);
            std::fs::remove_file(&path).unwrap();
            assert_eq!(package.index(0).unwrap_err().message, fallback);
        }
    }

    #[test]
    fn modular_root_objective_spans_require_matching_original_source() {
        for (declaration, reference, message) in [
            (
                "objective = { anchor = \"1/missing\", disclosed = true, continue_play = false }",
                "\"1/missing\"",
                "Invalid objective anchor reference",
            ),
            (
                "objective = { anchor = \"1/start\", item = 999, disclosed = true, continue_play = false }",
                "999",
                "Invalid objective item reference",
            ),
        ] {
            let (root, _) = checked_module("namespace = \"demo\"\n");
            let path = root.path().join("scenario.toml");
            let original = std::fs::read_to_string(&path).unwrap();
            let authored = format!("{original}\n{declaration}\n");
            std::fs::write(&path, &authored).unwrap();
            let package = read_package(root.path()).unwrap();
            assert!(package.manifest_text.is_none());
            let line = authored
                .lines()
                .position(|line| line == declaration)
                .unwrap()
                + 1;
            let column = declaration.find(reference).unwrap() + 1;
            assert_eq!(
                package.check().unwrap_err().message,
                format!("scenario.toml:{line}:{column}: objective: {message}")
            );

            // Retain the pinned definition, but do not attach coordinates to a
            // changed or unavailable authoring file, even if its value still matches.
            std::fs::write(&path, format!("# inserted after resolution\n{authored}")).unwrap();
            let without_span = format!("scenario.toml: objective: {message}");
            assert_eq!(package.check().unwrap_err().message, without_span);
            std::fs::remove_file(&path).unwrap();
            assert_eq!(package.check().unwrap_err().message, without_span);
        }
    }

    #[test]
    fn module_semantic_origins_use_verified_source_and_never_guess_changed_spans() {
        for catalog in ["archetypes", "items"] {
            let (root, package) = checked_module(&format!(
                "namespace = \"demo\"\n[{catalog}.\"demo.bad\"]\nname = \"bad\"\nappearance_pool = \"missing\"\n"
            ));
            let failure = package.appearance_mapping(0).unwrap_err();
            assert!(
                failure.message.starts_with("catalog.toml:4:"),
                "{}",
                failure.message
            );
            assert!(failure.message.contains("archetype \"demo.bad\""));
            std::fs::write(root.path().join("catalog.toml"), "# changed source\n").unwrap();
            let failure = package.appearance_mapping(0).unwrap_err();
            assert!(
                failure.message.starts_with("catalog.toml: archetype"),
                "{}",
                failure.message
            );
        }
    }

    #[test]
    fn module_catalog_rule_failures_identify_species_and_templates() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/mob-arena");
        let source = resolve_source(&fixture).unwrap();
        let mut species = source
            .manifest
            .creatures
            .species
            .values()
            .next()
            .unwrap()
            .clone();
        species.kind = "missing_type".into();
        let catalog = crate::creature_authoring::Catalog {
            species: BTreeMap::from([("demo.bad".into(), species)]),
            ..Default::default()
        };
        let module = toml::to_string(&BTreeMap::from([
            ("namespace", toml::Value::String("demo".into())),
            ("creatures", toml::Value::try_from(catalog).unwrap()),
        ]))
        .unwrap();
        let (_root, package) = checked_module(&module);
        assert!(package
            .content()
            .unwrap_err()
            .message
            .starts_with("catalog.toml: species \"demo.bad\":"));
        let (_root, package) = checked_module(
            "namespace = \"demo\"\n[creatures.templates.\"demo.bad\"]\npriority = 0\nkind = \"missing_type\"\n",
        );
        assert!(package
            .content()
            .unwrap_err()
            .message
            .starts_with("catalog.toml: template \"demo.bad\":"));
    }

    #[test]
    fn topology_modules_resolve_compact_typed_maps_and_retain_ownership() {
        let module = r#"namespace = "demo"
[generation_recipes."demo.rooms"]
kind = "grid_rooms"
version = 1
room_width = [4, 6]
room_height = [3, 5]
extra_connections = [0, 1]
[procedural_worlds."demo.caves"]
kind = "grid_chain"
version = 1
base_region = 100
size = [12, 10, 2]
grid = [1, 1]
recipe = "demo.rooms"
domain = { kind = "open", max_depth = 1000000 }
[generation_groups."demo.authored_group"]
depth = 1
recipe = "demo.rooms"
grid = [1, 1]
members = [50]
[stair_pairs."demo.entry"]
upper = "1/down"
lower = "100/up"
"#;
        let root = source_package("[\"world.toml\"]");
        std::fs::write(root.path().join("world.toml"), module).unwrap();
        let resolved = resolve_source(root.path()).unwrap();
        assert_eq!(resolved.manifest.procedural_worlds.len(), 1);
        assert_eq!(resolved.manifest.generation_groups.len(), 1);
        assert_eq!(resolved.manifest.stair_pairs.len(), 1);
        assert_eq!(
            resolved.provenance["procedural_worlds.demo.caves"],
            "world.toml"
        );
        assert!(resolved.manifest_text.len() < 4096);
        std::fs::write(
            root.path().join("world.toml"),
            module.replace("recipe = \"demo.rooms\"", "recipe = \"demo.missing\""),
        )
        .unwrap();
        let invalid = resolve_source(root.path()).unwrap();
        assert_eq!(
            invalid.manifest.procedural_worlds["demo.caves"].recipe,
            "demo.missing"
        );
        // Resolution retains references; the ordinary package proof rejects unknown targets.
    }

    #[test]
    fn topology_module_unknown_recipe_names_its_definition_source() {
        let (_root, package) = checked_module(
            r#"namespace = "demo"
[procedural_worlds."demo.caves"]
kind = "grid_chain"
version = 1
base_region = 100
size = [12, 10, 2]
grid = [1, 1]
recipe = "demo.missing"
domain = { kind = "finite", count = 3 }
"#,
        );
        let message = package.check().unwrap_err().message;
        assert!(message.contains("catalog.toml"), "{message}");
        assert!(message.contains("demo.caves"), "{message}");
        assert!(message.contains("recipe"), "{message}");
    }

    #[test]
    fn typed_catalog_views_lower_without_accepting_irrelevant_fields_or_duplicates() {
        let root = source_package("[]");
        let file = root.path().join("scenario.toml");
        let original = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, format!("{original}\n[actors.guard]\nturn_ticks = 100\n[items.coin]\nname = \"Coin\"\nstackable = true\n")).unwrap();
        let resolved = resolve_source(root.path()).unwrap();
        assert_eq!(resolved.manifest.archetypes["guard"].turn_ticks, Some(100));
        assert!(resolved.manifest.archetypes["coin"].stackable);
        assert!(resolved.provenance.contains_key("actors.guard"));
        assert!(resolved.provenance.contains_key("items.coin"));
        for declaration in [
            "[actors.bad]\nstackable=true",
            "[actors.bad]\nreadable={identify=\"potion\",discovery=\"on_use\"}",
            "[actors.bad]\ndevice={ability=\"ray\",initial_charges={minimum=3,maximum=7},discovery=\"on_use\"}",
            "[actors.bad]\nname=\"Unsupported actor name\"",
            "[items.bad]\nturn_ticks=100",
            "[items.token]\nname=\"Duplicate\"",
            "[actors.shared]\nturn_ticks=100\n[items.shared]\nname=\"b\"",
        ] {
            std::fs::write(&file, format!("{original}\n{declaration}\n")).unwrap();
            assert!(resolve_source(root.path()).is_err(), "{declaration}");
        }
    }

    #[test]
    fn typed_item_catalog_preserves_magic_capabilities_and_source_provenance() {
        let root = source_package("[\"magic.toml\"]");
        std::fs::write(root.path().join("magic.toml"), r#"
            namespace = "magic"
            [items."magic.scroll"]
            name = "readable tablet"
            class = "tool"
            readable = { identify = "wand", discovery = "on_use" }
            [items."magic.device"]
            name = "charged stone"
            class = "gem"
            device = { ability = "magic.ray", initial_charges = { minimum = 3, maximum = 7 }, discovery = "on_use" }
        "#).unwrap();
        let resolved = resolve_source(root.path()).unwrap();
        let readable = resolved.manifest.archetypes["magic.scroll"]
            .readable
            .as_ref()
            .unwrap();
        assert_eq!(readable.identify, Some(ItemClass::Wand));
        assert_eq!(readable.discovery, ItemDiscovery::OnUse);
        let device = resolved.manifest.archetypes["magic.device"]
            .device
            .as_ref()
            .unwrap();
        assert_eq!(device.ability, "magic.ray");
        assert_eq!(
            device.initial_charges,
            ChargeRange {
                minimum: 3,
                maximum: 7
            }
        );
        assert_eq!(resolved.provenance["items.magic.scroll"], "magic.toml");
        assert_eq!(resolved.provenance["items.magic.device"], "magic.toml");
        // Normalization preserves capability even though neither item uses the
        // conventional Scroll or Wand physical class.
        let restored: Manifest = parse(&resolved.manifest_text, "resolved").unwrap();
        assert_eq!(
            restored.archetypes["magic.scroll"].readable.as_ref(),
            Some(readable)
        );
        assert_eq!(
            restored.archetypes["magic.device"].device.as_ref(),
            Some(device)
        );
    }

    #[test]
    fn empty_includes_are_not_persisted_in_resolved_source() {
        let root = source_package("[]");
        let resolved = resolve_source(root.path()).unwrap();
        assert!(!resolved.manifest_text.contains("modules"));
        let _: Manifest = parse(&resolved.manifest_text, "resolved").unwrap();
        assert_eq!(resolved.source_hashes.len(), 1);
    }

    #[test]
    fn resolution_order_is_semantic_free_but_raw_comments_change_integrity() {
        let root = source_package("[\"a.toml\", \"b.toml\"]");
        for (file, id) in [("a.toml", "a"), ("b.toml", "b")] {
            std::fs::write(
                root.path().join(file),
                format!("namespace = \"demo\"\n[archetypes.\"demo.{id}\"]\nname = \"Token\"\n"),
            )
            .unwrap();
        }
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        std::fs::copy(fixture.join("rules.toml"), root.path().join("rules.toml")).unwrap();
        std::fs::create_dir(root.path().join("regions")).unwrap();
        for file in std::fs::read_dir(fixture.join("regions")).unwrap() {
            let file = file.unwrap();
            std::fs::copy(
                file.path(),
                root.path().join("regions").join(file.file_name()),
            )
            .unwrap();
        }
        let first = resolve_source(root.path()).unwrap();
        let first_package = read_package(root.path()).unwrap();
        let manifest = root.path().join("scenario.toml");
        let source = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(
            &manifest,
            source.replace("[\"a.toml\", \"b.toml\"]", "[\"b.toml\", \"a.toml\"]"),
        )
        .unwrap();
        let reordered = resolve_source(root.path()).unwrap();
        assert_eq!(first.manifest_text, reordered.manifest_text);
        assert_ne!(first.source_hashes, reordered.source_hashes);
        let reordered_package = read_package(root.path()).unwrap();
        assert_eq!(
            first_package.certificate.model_hash,
            reordered_package.certificate.model_hash
        );
        assert_ne!(
            first_package.certificate.content_hash,
            reordered_package.certificate.content_hash
        );
        let file = root.path().join("a.toml");
        std::fs::write(
            &file,
            format!("{}\n# comment\n", std::fs::read_to_string(&file).unwrap()),
        )
        .unwrap();
        let commented = resolve_source(root.path()).unwrap();
        assert_eq!(reordered.manifest_text, commented.manifest_text);
        assert_ne!(reordered.source_hashes, commented.source_hashes);
        let commented_package = read_package(root.path()).unwrap();
        assert_eq!(
            first_package.certificate.model_hash,
            commented_package.certificate.model_hash
        );
        assert_ne!(
            reordered_package.certificate.content_hash,
            commented_package.certificate.content_hash
        );
        assert_eq!(
            first_package.build(42, false).unwrap(),
            commented_package.build(42, false).unwrap()
        );
    }

    #[test]
    fn source_rejects_escape_duplicate_unknown_and_nested_includes() {
        for includes in [
            "[\"../outside.toml\"]",
            "[\"scenario.toml\"]",
            "[\"a.toml\", \"a.toml\"]",
        ] {
            let root = source_package(includes);
            std::fs::write(root.path().join("a.toml"), "namespace = \"demo\"\n").unwrap();
            assert!(resolve_source(root.path()).is_err(), "{includes}");
        }
        for extra in ["modules = []", "script = \"run.py\""] {
            let root = source_package("[\"a.toml\"]");
            std::fs::write(
                root.path().join("a.toml"),
                format!("namespace = \"demo\"\n{extra}\n"),
            )
            .unwrap();
            assert!(resolve_source(root.path())
                .unwrap_err()
                .message
                .contains("a.toml"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn module_symlinks_are_rejected_even_inside_package() {
        let root = source_package("[\"alias.toml\"]");
        std::fs::write(root.path().join("real.toml"), "namespace = \"demo\"\n").unwrap();
        std::os::unix::fs::symlink("real.toml", root.path().join("alias.toml")).unwrap();
        assert!(resolve_source(root.path())
            .unwrap_err()
            .message
            .contains("symlink"));
    }

    #[test]
    fn module_namespace_and_duplicate_contract() {
        let mut target = BTreeMap::new();
        let mut origins = BTreeMap::new();
        merge(
            &mut target,
            BTreeMap::from([("demo.actor".into(), 1)]),
            "archetypes",
            "demo",
            "a.toml",
            &mut origins,
        )
        .unwrap();
        assert_eq!(origins["archetypes.demo.actor"], "a.toml");
        assert!(merge(
            &mut target,
            BTreeMap::from([("demo.actor".into(), 2)]),
            "archetypes",
            "demo",
            "b.toml",
            &mut origins
        )
        .is_err());
        assert!(merge(
            &mut target,
            BTreeMap::from([("foreign.actor".into(), 2)]),
            "archetypes",
            "demo",
            "b.toml",
            &mut origins
        )
        .is_err());
        assert!(!namespace("demo..actor"));
    }
}
