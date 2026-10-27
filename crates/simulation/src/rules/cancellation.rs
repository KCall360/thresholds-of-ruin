//! Preserve ordinary once-per-flag expansion while retaining cancellation origin.
//! The caller validates the same source and expanded-modifier bounds as add_source.
use super::RulesetError;
use crate::content_ids::FlagId;
use crate::rules::{Modifier, ModifierOp, Ruleset};
use std::collections::{BTreeMap, BTreeSet};

/// Each flag is classified by its least restricted reachable provenance. A flag
/// reached from BOTH tagged and untagged parents must survive cancellation.
/// Classification never changes the ordinary multiplicity/order of expansion.
pub(crate) fn expand_cancellable_source(
    modifiers: &[Modifier],
    rules: &Ruleset,
) -> Result<Vec<Modifier>, RulesetError> {
    rules.expanded_modifier_count(modifiers)?;
    let mut flags: BTreeMap<FlagId, bool> = BTreeMap::new();
    let mut pending = Vec::new();
    for modifier in modifiers {
        if let ModifierOp::Flag { flag } = &modifier.op {
            pending.push((flag.clone(), modifier.cancellable));
        }
    }
    while let Some((flag, cancellable)) = pending.pop() {
        match flags.get(&flag) {
            Some(false) => continue,
            Some(true) if cancellable => continue,
            _ => {}
        }
        flags.insert(flag.clone(), cancellable);
        let definition = rules.flag(&flag).ok_or(RulesetError::Bound {
            field: "cancellation_source".into(),
        })?;
        for modifier in &definition.modifiers {
            if let ModifierOp::Flag { flag } = &modifier.op {
                pending.push((flag.clone(), cancellable || modifier.cancellable));
            }
        }
    }
    let mut expanded = modifiers.to_vec();
    let mut seen = BTreeSet::new();
    let mut cursor = 0;
    while cursor < expanded.len() {
        if let ModifierOp::Flag { flag } = &expanded[cursor].op {
            if seen.insert(flag.clone()) {
                let inherited = *flags.get(flag).ok_or(RulesetError::Bound {
                    field: "cancellation_source".into(),
                })?;
                let definition = rules.flag(flag).ok_or(RulesetError::Bound {
                    field: "cancellation_source".into(),
                })?;
                expanded.extend(definition.modifiers.iter().cloned().map(|mut modifier| {
                    modifier.cancellable |= inherited;
                    modifier
                }));
            }
        }
        cursor += 1;
    }
    Ok(expanded)
}

/// A source-reachability owner decides whether Mixed is forbidden for a package.
/// Merely importing an unused definition does not activate cancellation policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancellationInvisibilityShape {
    Absent,
    Pure,
    Mixed,
}
impl Ruleset {
    pub fn cancellation_invisibility_shape(
        &self,
        id: &crate::content_ids::ConditionId,
    ) -> Result<CancellationInvisibilityShape, RulesetError> {
        let definition = self
            .condition(id)
            .ok_or_else(|| RulesetError::UnknownReference {
                category: "condition",
                id: id.to_string(),
            })?;
        let modifiers = expand_cancellable_source(&definition.modifiers, self)?;
        let invisible = |modifier: &Modifier| {
            matches!(
                modifier.op,
                ModifierOp::Perception {
                    value: crate::perception::PerceptionValue::Invisible { enabled: true }
                }
            )
        };
        if !modifiers.iter().any(invisible) {
            return Ok(CancellationInvisibilityShape::Absent);
        }
        // A flag can itself be a sustaining/requirement identity. Do not erase
        // it merely because its current implications happen to be invisible.
        let pure = definition.restrictions.is_empty()
            && modifiers.iter().all(|modifier| {
                modifier.scope == crate::rules::Scope::Always && invisible(modifier)
            });
        Ok(if pure {
            CancellationInvisibilityShape::Pure
        } else {
            CancellationInvisibilityShape::Mixed
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{FlagDef, Scope, Stat};
    fn flag(name: &str, cancellable: bool) -> Modifier {
        Modifier {
            cancellable,
            scope: Scope::Always,
            op: ModifierOp::Flag {
                flag: FlagId::new(name).unwrap(),
            },
        }
    }
    fn fixture() -> std::sync::Arc<Ruleset> {
        let mut document = crate::test_creatures::ruleset().document().clone();
        document.flags.extend([
            FlagDef {
                id: FlagId::new("cancel_leaf").unwrap(),
                name: "leaf".into(),
                modifiers: vec![Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::AttackDamageRollBonus,
                        value: 7,
                    },
                }],
            },
            FlagDef {
                id: FlagId::new("cancel_left").unwrap(),
                name: "left".into(),
                modifiers: vec![flag("cancel_leaf", false)],
            },
            FlagDef {
                id: FlagId::new("cancel_right").unwrap(),
                name: "right".into(),
                modifiers: vec![flag("cancel_leaf", false)],
            },
        ]);
        Ruleset::validate(document).unwrap()
    }
    #[test]
    fn shared_flag_keeps_unmarked_path_and_original_single_contribution() {
        let rules = fixture();
        for reverse in [false, true] {
            let mut source = vec![flag("cancel_left", true), flag("cancel_right", false)];
            if reverse {
                source.reverse();
            }
            let expanded = expand_cancellable_source(&source, &rules).unwrap();
            let leaf: Vec<_> = expanded
                .iter()
                .filter(|m| {
                    matches!(
                        m.op,
                        ModifierOp::Add {
                            stat: Stat::AttackDamageRollBonus,
                            value: 7
                        }
                    )
                })
                .collect();
            assert_eq!(
                leaf.len(),
                1,
                "ordinary diamond expansion stays once per flag"
            );
            assert!(
                !leaf[0].cancellable,
                "later unmarked path must rescue shared implication"
            );
        }
        let expanded = expand_cancellable_source(
            &[flag("cancel_left", true), flag("cancel_right", true)],
            &rules,
        )
        .unwrap();
        assert!(expanded
            .iter()
            .filter(|m| matches!(
                m.op,
                ModifierOp::Add {
                    stat: Stat::AttackDamageRollBonus,
                    value: 7
                }
            ))
            .all(|m| m.cancellable));
    }
    #[test]
    fn cancellation_metadata_does_not_relax_source_cycle_rejection() {
        let rules = fixture();
        let mut document = rules.document().clone();
        document
            .flags
            .iter_mut()
            .find(|f| f.id.as_str() == "cancel_leaf")
            .unwrap()
            .modifiers
            .push(flag("cancel_left", true));
        assert!(Ruleset::validate(document).is_err());
    }
}
