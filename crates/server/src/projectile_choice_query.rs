//! Read-only structural owner menus, bounded in scanned inventory rows.
use super::{ActorId, BranchId, Engine, Failure};
use tor_protocol as p;
use tor_simulation::{self as s, abilities as a};

pub(crate) struct PageBudget<'a> {
    pub context: &'a p::ReplyContext,
    pub request_id: &'a str,
    pub limit: usize,
}
impl PageBudget<'_> {
    fn fits(&self, page: &p::ProjectileChoicePage) -> Result<bool, Failure> {
        #[derive(serde::Serialize)]
        struct Envelope<'a> {
            #[serde(rename = "type")]
            kind: &'static str,
            context: &'a p::ReplyContext,
            request_id: &'a str,
            page: &'a p::ProjectileChoicePage,
        }
        p::encoded_length(
            &Envelope {
                kind: "projectile_choices",
                context: self.context,
                request_id: self.request_id,
                page,
            },
            self.limit.min(p::MAX_PROJECTILE_CHOICE_PAGE_BYTES),
        )
        .map(|size| size.is_some())
        .map_err(|_| unavailable())
    }
}
fn page_limit() -> Failure {
    Failure::new(
        p::ErrorCode::ResourceLimit,
        "Projectile choices cannot fit the response limit",
    )
}
fn unavailable() -> Failure {
    Failure::new(
        p::ErrorCode::InvalidAction,
        "Projectile choices are unavailable",
    )
}
impl Engine {
    pub(crate) fn projectile_choices(
        &self,
        actor: ActorId,
        branch: &BranchId,
        revision: u64,
        query: &p::ProjectileChoiceQuery,
        budget: &PageBudget<'_>,
    ) -> Result<p::ProjectileChoicePage, Failure> {
        self.check_metadata(
            actor,
            branch,
            Some((revision, "Refresh the observation before choosing")),
        )?;
        if !query.is_valid() {
            return Err(Failure::new(
                p::ErrorCode::InvalidRequest,
                "Invalid projectile choice query",
            ));
        }
        let ability = s::content_ids::AbilityId::new(&query.ability).map_err(|_| unavailable())?;
        let view = self.revision_view(actor)?;
        let observation = &view.observation;
        let inventory = &observation.inventory;
        // The wire cursor never truncates a host collection length or wraps.
        let total = u32::try_from(inventory.len()).map_err(|_| {
            Failure::new(
                p::ErrorCode::ResourceLimit,
                "Projectile inventory exceeds cursor capacity",
            )
        })?;
        if query.offset > total {
            return Err(Failure::new(
                p::ErrorCode::InvalidRequest,
                "Invalid projectile choice offset",
            ));
        }
        let scope = self.target_scope(actor);
        let owner = s::ActorId(actor.0);
        let domain = match query.phase {
            p::ProjectileChoicePhase::Sources {} => {
                let creature = self.game.creature(owner).ok_or_else(unavailable)?;
                if !creature.derived().abilities.contains(&ability) {
                    return Err(unavailable());
                }
                let definition = self
                    .game
                    .content()
                    .rules()
                    .ability(&ability)
                    .ok_or_else(unavailable)?;
                match definition.strikes {
                    s::rules::StrikeSource::Projectile {
                        delivery: s::projectile_profiles::ProjectileDeliveryKind::Throw,
                    } => a::ProjectileChoiceQuery::ThrowSources,
                    s::rules::StrikeSource::Projectile {
                        delivery: s::projectile_profiles::ProjectileDeliveryKind::Launch,
                    } => a::ProjectileChoiceQuery::Launchers,
                    _ => return Err(unavailable()),
                }
            }
            p::ProjectileChoicePhase::Ammunition { launcher } => {
                // Exact owner-token resolution follows existing linear disclosure
                // lookup. The 128-row bound applies to candidate evaluation below.
                // Type knowledge gates recommendations independently of exact-action
                // authority. Never infer profiles/families from concealed class or socket.
                let item = inventory
                    .iter()
                    .find(|item| item.identified && scope.item(item.id) == launcher.item)
                    .ok_or_else(unavailable)?;
                let interactions = observation.interactions.as_ref().ok_or_else(unavailable)?;
                if usize::from(launcher.socket) >= interactions.slots.len()
                    || !interactions.inventory.iter().any(|offer| {
                        offer.item == item.id
                            && offer.equipped_slot == Some(s::EquipmentSlotId(launcher.socket))
                    })
                {
                    return Err(unavailable());
                }
                let launcher = a::EquippedProjectileWeapon {
                    item: item.id,
                    socket: s::EquipmentSlotId(launcher.socket),
                };
                if self.game.projectile_choice_for_item(
                    owner,
                    &ability,
                    a::ProjectileChoiceQuery::Launchers,
                    item.id,
                ) != Some(a::ProjectileChoice::Launcher { launcher })
                {
                    return Err(unavailable());
                }
                a::ProjectileChoiceQuery::Ammunition { launcher }
            }
        };
        let rows = inventory.iter().skip(query.offset as usize).map(|item| {
            // Type knowledge only: no numeric/instance identification requirement.
            if !item.identified {
                return None;
            }
            self.game
                .projectile_choice_for_item(owner, &ability, domain, item.id)
                .map(|choice| match choice {
                    a::ProjectileChoice::Throw { source } => p::ProjectileChoice::Throw {
                        source: match source {
                            a::ThrownSourceSelection::Equipped { item, socket } => {
                                p::ThrownSourceSelection::Equipped {
                                    item: scope.item(item),
                                    socket: socket.0,
                                }
                            }
                            a::ThrownSourceSelection::Carried { item, socket } => {
                                p::ThrownSourceSelection::Carried {
                                    item: scope.item(item),
                                    socket: socket.0,
                                }
                            }
                        },
                    },
                    a::ProjectileChoice::Launcher { launcher } => p::ProjectileChoice::Launcher {
                        launcher: p::EquippedProjectileWeapon {
                            item: scope.item(launcher.item),
                            socket: launcher.socket.0,
                        },
                    },
                    a::ProjectileChoice::Ammunition { item } => p::ProjectileChoice::Ammunition {
                        item: scope.item(item),
                    },
                })
        });
        pack(query, total, rows, budget)
    }
}

/// Bound traversal before evaluating another candidate. Filtered rows consume the
/// same scan budget as emitted rows, and response-budget truncation retries the
/// un-emitted candidate on the next page.
fn pack(
    query: &p::ProjectileChoiceQuery,
    total: u32,
    rows: impl IntoIterator<Item = Option<p::ProjectileChoice>>,
    budget: &PageBudget<'_>,
) -> Result<p::ProjectileChoicePage, Failure> {
    let mut page = p::ProjectileChoicePage {
        query: query.clone(),
        choices: Vec::new(),
        next_offset: None,
    };
    let mut end = query.offset;
    for choice in rows.into_iter().take(p::MAX_PROJECTILE_CHOICE_SCAN) {
        end = end
            .checked_add(1)
            .filter(|end| *end <= total)
            .ok_or_else(unavailable)?;
        let previous_choices = page.choices.len();
        if let Some(choice) = choice {
            page.choices.push(choice);
        }
        page.next_offset = (end < total).then_some(end);
        // Filtered rows can widen the cursor (including numeric to null at the
        // end). Keep the last fitting boundary instead of skipping that row or
        // rejecting a previously fitting prefix.
        if !budget.fits(&page)? {
            page.choices.truncate(previous_choices);
            end -= 1;
            if end == query.offset {
                return Err(page_limit());
            }
            break;
        }
        if page.choices.len() == p::MAX_PROJECTILE_CHOICES {
            break;
        }
    }
    page.next_offset = (end < total).then_some(end);
    if !page.valid_for(query) {
        return Err(unavailable());
    }
    if !budget.fits(&page)? {
        return Err(page_limit());
    }
    Ok(page)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    fn context() -> p::ReplyContext {
        serde_json::from_value(serde_json::json!({"input":{"stream":{"stream":"attachment","epoch":"0"},"readiness_revision":"0"},"actor":"1","branch":"main","cursor":{"sequence":"0","tick":"0"},"revision":"0"})).unwrap()
    }
    fn query(offset: u32) -> p::ProjectileChoiceQuery {
        p::ProjectileChoiceQuery {
            ability: "fixture.launch".into(),
            phase: p::ProjectileChoicePhase::Ammunition {
                launcher: p::EquippedProjectileWeapon {
                    item: p::ItemTarget::from_digest([0; 32]),
                    socket: 63,
                },
            },
            offset,
        }
    }
    fn row(index: u8) -> p::ProjectileChoice {
        p::ProjectileChoice::Ammunition {
            item: p::ItemTarget::from_digest([index; 32]),
        }
    }
    #[test]
    fn projectile_page_scan_stops_at_128_and_empty_filtered_page_makes_progress() {
        let context = context();
        let budget = PageBudget {
            context: &context,
            request_id: "choices",
            limit: p::MAX_PROJECTILE_CHOICE_PAGE_BYTES,
        };
        let visited = Cell::new(0);
        let page = pack(
            &query(0),
            129,
            (0..129).map(|index| {
                visited.set(visited.get() + 1);
                (index == 128).then(|| row(1))
            }),
            &budget,
        )
        .unwrap();
        assert_eq!(visited.get(), 128, "candidate 129 is not evaluated");
        assert!(page.choices.is_empty());
        assert_eq!(page.next_offset, Some(128));
        let final_page = pack(&query(128), 129, [Some(row(1))], &budget).unwrap();
        assert_eq!(final_page.choices, [row(1)]);
        assert_eq!(final_page.next_offset, None);
        let visited = Cell::new(0);
        let page = pack(
            &query(0),
            100,
            (1..=100).map(|index| {
                visited.set(visited.get() + 1);
                Some(row(index))
            }),
            &budget,
        )
        .unwrap();
        assert_eq!(visited.get(), 16);
        assert_eq!(page.choices.len(), 16);
        assert_eq!(page.next_offset, Some(16));
    }
    #[test]
    fn projectile_page_byte_budget_retries_unemitted_row_without_skipping_it() {
        let context = context();
        let reference = p::ServerMessage::ProjectileChoices {
            context: context.clone(),
            request_id: "choices".into(),
            page: Box::new(p::ProjectileChoicePage {
                query: query(0),
                choices: vec![row(1)],
                next_offset: Some(3),
            }),
        };
        let limit = p::encoded_length(&reference, usize::MAX).unwrap().unwrap();
        let budget = PageBudget {
            context: &context,
            request_id: "choices",
            limit,
        };
        let first = pack(
            &query(0),
            3,
            [Some(row(1)), Some(row(2)), Some(row(3))],
            &budget,
        )
        .unwrap();
        assert_eq!(first.choices, [row(1)]);
        assert_eq!(first.next_offset, Some(1));
        let second = pack(&query(1), 3, [Some(row(2)), Some(row(3))], &budget).unwrap();
        assert_eq!(second.choices, [row(2)]);
        assert_eq!(second.next_offset, Some(2));
        let third = pack(
            &query(2),
            3,
            [Some(row(3))],
            &PageBudget {
                limit: limit + 3,
                ..budget
            },
        )
        .unwrap();
        assert_eq!(third.choices, [row(3)]);
        assert_eq!(third.next_offset, None);
    }
    #[test]
    fn projectile_filtered_tail_retains_fitting_continuation_when_null_is_longer() {
        let context = context();
        let reference = p::ServerMessage::ProjectileChoices {
            context: context.clone(),
            request_id: "choices".into(),
            page: Box::new(p::ProjectileChoicePage {
                query: query(0),
                choices: vec![row(1)],
                next_offset: Some(1),
            }),
        };
        let limit = p::encoded_length(&reference, usize::MAX).unwrap().unwrap();
        let budget = PageBudget {
            context: &context,
            request_id: "choices",
            limit,
        };
        let first = pack(&query(0), 3, [Some(row(1)), None, None], &budget).unwrap();
        assert_eq!(first.choices, [row(1)]);
        assert_eq!(
            first.next_offset,
            Some(2),
            "last fitting cursor before null widens"
        );
        let last = pack(&query(2), 3, [None], &budget).unwrap();
        assert!(last.choices.is_empty() && last.next_offset.is_none());
    }
    #[test]
    fn projectile_singleton_with_maximum_escaped_metadata_fits_two_kib() {
        let mut context = context();
        context.input.stream.stream.0 = "\\\"".repeat(64);
        context.branch.0 = "\\\"".repeat(64);
        context.input.stream.epoch = u64::MAX;
        context.input.readiness_revision = u64::MAX;
        context.revision = u64::MAX;
        context.actor.0 = u64::MAX;
        context.cursor.sequence = u64::MAX;
        context.cursor.tick = u64::MAX;
        let request_id = "\\\"".repeat(64);
        let mut query = query(u32::MAX - 1);
        query.ability = "a".repeat(60);
        let budget = PageBudget {
            context: &context,
            request_id: &request_id,
            limit: 2048,
        };
        let page = pack(&query, u32::MAX, [Some(row(1))], &budget).unwrap();
        assert_eq!(page.choices, [row(1)]);
        let text = p::encode_response(
            &p::ServerMessage::ProjectileChoices {
                context,
                request_id,
                page: Box::new(page),
            },
            None,
            2048,
        )
        .unwrap()
        .text;
        assert!(p::decode_response(&text).is_ok());
    }
}
