//! Host-side shared rules imports. Runtime and saves use the resolved immutable
//! document; imports never consult the network or arbitrary filesystem paths.
use super::{fail, parse, require, source_digest, Failure, MAX_BYTES, RULESET};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tor_simulation::rules::{RulesetDocument, DEFAULT_RULESET_TOML};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Import {
    import: String,
    #[serde(default)]
    overrides: Vec<Override>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Override {
    Set {
        path: Vec<String>,
        value: toml::Value,
    },
    Add {
        path: Vec<String>,
        value: toml::Value,
    },
    Remove {
        path: Vec<String>,
    },
}
impl Override {
    fn path(&self) -> &[String] {
        match self {
            Self::Set { path, .. } | Self::Add { path, .. } | Self::Remove { path } => path,
        }
    }
}
pub(super) struct Resolved {
    pub document: RulesetDocument,
    pub dependencies: BTreeMap<String, String>,
    pub materialized: Option<String>,
}
pub(super) fn resolve(source: &str, file: &str) -> Result<Resolved, Failure> {
    require(
        source.len() as u64 <= MAX_BYTES,
        "Ruleset exceeds source limit",
    )?;
    let value: toml::Value = parse(source, file)?;
    if value.get("import").is_none() {
        return Ok(Resolved {
            document: parse(source, file)?,
            dependencies: BTreeMap::new(),
            materialized: None,
        });
    }
    let input: Import = parse(source, file)?;
    require(input.import == RULESET, "Unknown shared rules import")?;
    require(input.overrides.len() <= 4096, "Too many rules overrides")?;
    let mut value: toml::Value = parse(DEFAULT_RULESET_TOML, "shared rules")?;
    let mut paths: Vec<&[String]> = Vec::new();
    for change in &input.overrides {
        let path = change.path();
        require(
            !path.is_empty() && path.len() <= 32 && path[0] != "id",
            "Invalid rules override path",
        )?;
        require(
            path.iter().all(|s| !s.is_empty() && s.len() <= 128),
            "Invalid rules override segment",
        )?;
        require(
            !paths
                .iter()
                .any(|previous| previous.starts_with(path) || path.starts_with(previous)),
            "Overlapping rules overrides",
        )?;
        paths.push(path);
        apply(&mut value, path, change, false).map_err(|e| {
            fail(format!(
                "{file}: override {}: {}",
                path.join("."),
                e.message
            ))
        })?;
    }
    let materialized = toml::to_string(&value).map_err(|e| fail(e.to_string()))?;
    require(
        materialized.len() as u64 <= MAX_BYTES,
        "Resolved rules exceed source limit",
    )?;
    Ok(Resolved {
        document: parse(&materialized, file)?,
        dependencies: BTreeMap::from([(input.import, source_digest(DEFAULT_RULESET_TOML))]),
        materialized: Some(materialized),
    })
}
/// Tool-facing resolution uses exactly the loader's import and override engine.
#[derive(Debug, Serialize)]
pub struct ResolvedRules {
    pub document: toml::Value,
    pub source: String,
    pub dependencies: BTreeMap<String, String>,
}

/// Resolve authoring syntax. Full semantic validation belongs to package check.
pub fn resolve_rules_source(source: &str, file: &str) -> Result<ResolvedRules, Failure> {
    let resolved = resolve(source, file)?;
    let source = resolved.materialized.unwrap_or_else(|| source.to_owned());
    Ok(ResolvedRules {
        document: parse(&source, file)?,
        source,
        dependencies: resolved.dependencies,
    })
}

fn identity(value: &toml::Value) -> Option<&str> {
    value
        .get("id")
        .or_else(|| value.get("resource"))
        .or_else(|| value.get("pool"))
        .and_then(toml::Value::as_str)
}
fn apply(
    current: &mut toml::Value,
    path: &[String],
    change: &Override,
    catalog_member: bool,
) -> Result<(), Failure> {
    let segment = &path[0];
    require(
        !catalog_member || !matches!(segment.as_str(), "id" | "resource" | "pool"),
        "Catalog identity must use explicit remove/add",
    )?;
    if path.len() > 1 {
        let child_is_catalog_member = matches!(current, toml::Value::Array(_));
        let child = match current {
            toml::Value::Table(table) => table.get_mut(segment),
            toml::Value::Array(values) => values
                .iter_mut()
                .find(|v| identity(v) == Some(segment.as_str())),
            _ => None,
        }
        .ok_or_else(|| fail("Unknown rules override path"))?;
        return apply(child, &path[1..], change, child_is_catalog_member);
    }
    match current {
        toml::Value::Table(table) => match change {
            Override::Set { value, .. } => {
                require(
                    table.contains_key(segment),
                    "Set must target an existing field",
                )?;
                table.insert(segment.clone(), value.clone());
            }
            Override::Add { value, .. } => {
                require(!table.contains_key(segment), "Add must target a new field")?;
                table.insert(segment.clone(), value.clone());
            }
            Override::Remove { .. } => {
                require(
                    table.remove(segment).is_some(),
                    "Remove must target an existing field",
                )?;
            }
        },
        toml::Value::Array(values) => {
            let position = values
                .iter()
                .position(|v| identity(v) == Some(segment.as_str()));
            match change {
                Override::Set { value, .. } => {
                    require(
                        identity(value) == Some(segment.as_str()),
                        "Replacement must retain content ID",
                    )?;
                    let index =
                        position.ok_or_else(|| fail("Set must target an existing content ID"))?;
                    values[index] = value.clone();
                }
                Override::Add { value, .. } => {
                    require(position.is_none(), "Add must target a new content ID")?;
                    require(
                        identity(value) == Some(segment.as_str()),
                        "Added definition must match content ID",
                    )?;
                    values.push(value.clone());
                }
                Override::Remove { .. } => {
                    let index = position
                        .ok_or_else(|| fail("Remove must target an existing content ID"))?;
                    values.remove(index);
                }
            }
        }
        _ => return Err(fail("Rules override parent is not a table or catalog")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(overrides: &str) -> String {
        format!("import = \"{RULESET}\"\n{overrides}")
    }
    #[test]
    fn supply_catalog_overrides_use_validated_stable_identities() {
        let resolved = resolve(
            &source(
                r#"
[[overrides]]
op = "set"
path = ["supplies", "bandage", "name"]
value = "Clean bandage"
[[overrides]]
op = "add"
path = ["supplies", "thread"]
value = { id = "thread", name = "Thread" }
"#,
            ),
            "rules.toml",
        )
        .unwrap();
        let rules = tor_simulation::rules::Ruleset::validate(resolved.document).unwrap();
        let id = tor_simulation::content_ids::SupplyId::new("bandage").unwrap();
        assert_eq!(rules.supply(&id).unwrap().name, "Clean bandage");
        let id = tor_simulation::content_ids::SupplyId::new("thread").unwrap();
        assert!(rules.supply(&id).is_some());
        let resolved = resolve(
            &source(
                r#"
[[overrides]]
op = "remove"
path = ["supplies", "bandage"]
"#,
            ),
            "rules.toml",
        )
        .unwrap();
        assert!(tor_simulation::rules::Ruleset::validate(resolved.document).is_err());
    }
    #[test]
    fn shared_import_is_deterministic_and_pins_dependency() {
        let first = resolve(&source(""), "rules.toml").unwrap();
        let second = resolve(&source(""), "rules.toml").unwrap();
        assert_eq!(first.materialized, second.materialized);
        assert_eq!(
            first.dependencies[RULESET],
            source_digest(DEFAULT_RULESET_TOML)
        );
        assert_eq!(
            first.document,
            parse::<RulesetDocument>(DEFAULT_RULESET_TOML, "canonical").unwrap()
        );
    }
    #[test]
    fn catalog_override_uses_stable_identity_not_position() {
        let resolved = resolve(
            &source(
                r#"
[[overrides]]
op = "set"
path = ["resources", "mana", "recovery_period"]
value = 777
"#,
            ),
            "rules.toml",
        )
        .unwrap();
        let resource = resolved
            .document
            .resources
            .iter()
            .find(|r| r.resource == tor_simulation::resources::Resource::Mana)
            .unwrap();
        assert_eq!(resource.recovery_period, 777);
        tor_simulation::rules::Ruleset::validate(resolved.document).unwrap();
    }
    #[test]
    fn catalog_add_and_remove_are_explicit() {
        let resolved = resolve(
            &source(
                r#"
[[overrides]]
op = "add"
path = ["descriptors", "scenario_mark"]
value = { id = "scenario_mark", name = "Scenario mark" }
[[overrides]]
op = "remove"
path = ["talents", "open_the_guard"]
"#,
            ),
            "rules.toml",
        )
        .unwrap();
        assert!(resolved
            .document
            .descriptors
            .iter()
            .any(|d| d.id.as_str() == "scenario_mark"));
        assert!(!resolved
            .document
            .talents
            .iter()
            .any(|d| d.id.as_str() == "open_the_guard"));
        tor_simulation::rules::Ruleset::validate(resolved.document).unwrap();
    }
    #[test]
    fn point_grant_overrides_use_pool_identity_and_cannot_rewrite_it() {
        let resolved = resolve(
            &source(
                r#"
[[overrides]]
op = "set"
path = ["constants", "racial_point_grants", "advancement", "points"]
value = 2
[[overrides]]
op = "set"
path = ["classes", "warrior", "point_grants", "advancement", "points"]
value = 3
"#,
            ),
            "rules.toml",
        )
        .unwrap();
        assert_eq!(resolved.document.constants.racial_point_grants[0].points, 2);
        assert_eq!(
            resolved
                .document
                .classes
                .iter()
                .find(|class| class.id.as_str() == "warrior")
                .unwrap()
                .point_grants[0]
                .points,
            3
        );
        for path in [
            "['constants','racial_point_grants','advancement','pool']",
            "['classes','warrior','point_grants','advancement','pool']",
        ] {
            assert!(resolve(
                &source(&format!(
                    "[[overrides]]\nop='set'\npath={path}\nvalue='renamed'"
                )),
                "rules.toml"
            )
            .is_err());
        }
    }
    #[test]
    fn invalid_or_ambiguous_override_paths_are_rejected() {
        for body in [
            "[[overrides]]\nop='set'\npath=['constants','missing']\nvalue=2",
            "[[overrides]]\nop='add'\npath=['constants','base_defense']\nvalue=2",
            "[[overrides]]\nop='remove'\npath=['talents','missing']",
            "[[overrides]]\nop='set'\npath=['resources','0','recovery_period']\nvalue=2",
            "[[overrides]]\nop='set'\npath=['id']\nvalue='changed'",
            "[[overrides]]\nop='add'\npath=['descriptors','new']\nvalue={id='wrong',name='Wrong'}",
            "[[overrides]]\nop='remove'\npath=['talents','open_the_guard']\nvalue=1",
            "[[overrides]]\nop='set'\npath=['constants','base_defense']\nvalue=2\n[[overrides]]\nop='set'\npath=['constants','base_defense']\nvalue=3",
            "[[overrides]]\nop='set'\npath=['constants','base_defense']\nvalue=2\n[[overrides]]\nop='remove'\npath=['constants']",
        ] {
            assert!(resolve(&source(body), "rules.toml").is_err(), "{body}");
        }
        for import in [
            "../rules.toml",
            "https://example.org/rules",
            "interactions-v26",
        ] {
            assert!(resolve(&format!("import = '{import}'"), "rules.toml").is_err());
        }
    }
    #[test]
    fn nested_catalog_identity_changes_cannot_bypass_stable_id_overrides() {
        for path in [
            "['talents','brace_for_impact','id']",
            "['resources','mana','resource']",
        ] {
            let input = source(&format!(
                "[[overrides]]\nop='set'\npath={path}\nvalue='renamed'"
            ));
            assert!(resolve(&input, "rules.toml").is_err());
        }
        let input = source(
            "[[overrides]]\nop='set'\npath=['abilities','spark_lance_fire','cost','resource']\nvalue='focus'",
        );
        let resolved = resolve(&input, "rules.toml").unwrap();
        tor_simulation::rules::Ruleset::validate(resolved.document).unwrap();
    }

    #[test]
    fn final_rules_validation_rejects_broken_cross_references_and_bounds() {
        for body in [
            "[[overrides]]\nop='remove'\npath=['abilities','spark_lance_fire']",
            "[[overrides]]\nop='set'\npath=['constants','base_defense']\nvalue=-999999999",
        ] {
            let resolved = resolve(&source(body), "rules.toml").unwrap();
            assert!(tor_simulation::rules::Ruleset::validate(resolved.document).is_err());
        }
    }
}

#[cfg(test)]
mod form_tests {
    use super::*;
    #[test]
    fn namespaced_form_imports_pin_dependencies_and_compiled_semantic_identity() {
        use tor_simulation::{content_ids::*, rules::*};
        let baseline: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
        let mut ability = baseline
            .abilities
            .iter()
            .find(|row| row.id.as_str() == "spark_lance_fire")
            .unwrap()
            .clone();
        ability.id = AbilityId::new("starter.spark").unwrap();
        let form = FormDef {
            id: FormId::new("starter.cold").unwrap(),
            name: "Cold Spark".into(),
            ability: ability.id.clone(),
        };
        let root = TalentDef {
            id: TalentId::new("fixture_form_root").unwrap(),
            name: "Fixture Form Root".into(),
            prerequisites: vec![],
            requirement: Requirement::TotalHd { minimum: 1 },
            modifiers: vec![],
            form_choices: vec![form.id.clone()],
        };
        let change = |op: &str, path: &[&str], value: toml::Value| {
            toml::Value::Table(toml::map::Map::from_iter([
                ("op".into(), toml::Value::String(op.into())),
                (
                    "path".into(),
                    toml::Value::Array(
                        path.iter()
                            .map(|part| toml::Value::String((*part).into()))
                            .collect(),
                    ),
                ),
                ("value".into(), value),
            ]))
        };
        let source = toml::to_string(&toml::Value::Table(toml::map::Map::from_iter([
            ("import".into(), toml::Value::String(RULESET.into())),
            (
                "overrides".into(),
                toml::Value::Array(vec![
                    change(
                        "add",
                        &["abilities", "starter.spark"],
                        toml::Value::try_from(&ability).unwrap(),
                    ),
                    change(
                        "add",
                        &["forms", "starter.cold"],
                        toml::Value::try_from(&form).unwrap(),
                    ),
                    change(
                        "add",
                        &["talents", "fixture_form_root"],
                        toml::Value::try_from(&root).unwrap(),
                    ),
                ]),
            ),
        ])))
        .unwrap();
        let resolved = resolve(&source, "rules.toml").unwrap();
        assert_eq!(
            resolved.dependencies[RULESET],
            source_digest(DEFAULT_RULESET_TOML)
        );
        let rules = Ruleset::validate(resolved.document).unwrap();
        assert_eq!(rules.form(&form.id).unwrap().ability, ability.id);
        assert_eq!(
            rules.talent(&root.id).unwrap().form_choices,
            vec![form.id.clone()]
        );
        let base = crate::advancement_test_support::scenario();
        let package = base.package.as_ref().unwrap();
        let compile = |text: &str| {
            crate::scenario_package::Package::from_parts(
                package.manifest.clone(),
                package.region_defs().unwrap(),
                text,
            )
            .unwrap()
        };
        let first = compile(&source);
        let second = compile(&source);
        assert_eq!(first.certificate.model_hash, second.certificate.model_hash);
        let changed = source.replace("Cold Spark", "Cold Spark renamed");
        assert_ne!(
            compile(&changed).certificate.model_hash,
            first.certificate.model_hash
        );
        let missing = source.replace("starter.cold", "starter.missing");
        let mut missing = resolve(&missing, "rules.toml").unwrap().document;
        missing
            .talents
            .iter_mut()
            .find(|row| row.id == root.id)
            .unwrap()
            .form_choices = vec![form.id];
        assert!(Ruleset::validate(missing).is_err());
    }
}
