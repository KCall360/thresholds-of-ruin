//! Bounded private strike-page navigation, separate from ordinary observations.
use std::collections::{BTreeMap, VecDeque};
use tor_protocol::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrikeSourceChoice {
    pub source: StrikeSourceSelector,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Source {
    choice: StrikeSourceChoice,
    resolved: StrikeSourceAttribution,
    count: u16,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    actor: ActorId,
    revision: u64,
    context: ReplyContext,
    sources: Vec<Source>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    cursor: Option<StrikePageCursor>,
    index: usize,
    request_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Rendered {
    cursor: StrikePageCursor,
    rows: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Active {
    source: Source,
    handle: Option<StrikeInspectionId>,
    current: usize,
    cursors: Vec<StrikePageCursor>,
    next: Option<StrikePageCursor>,
    rendered: VecDeque<Rendered>,
    structure: Structure,
}
/// Only typed cursors and compact source shapes survive eviction of a rendered page.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RiderShape {
    trigger: RiderTrigger,
    condition: Option<InspectionRiderCondition>,
    lengths: [u16; 2],
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Structure {
    strikes: BTreeMap<u16, [u16; 4]>,
    offsets: BTreeMap<u16, u64>,
    primary_checked: BTreeMap<u16, bool>,
    riders: BTreeMap<(u16, u16), RiderShape>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InspectionPages {
    target: Option<Target>,
    choices: Vec<StrikeSourceChoice>,
    active: Option<Active>,
    pending: Option<Pending>,
}
const STALE: &str = "Strike inspection changed. Reopen the creature inspection.";

impl InspectionPages {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn set_report(
        &mut self,
        report: &CreatureInspectionView,
        context: &ReplyContext,
    ) -> Result<(), String> {
        report.validate().map_err(|error| error.to_string())?;
        self.clear();
        let natural = StrikeSourceAttribution::Natural {
            species: report.natural_attack.source.clone(),
        };
        let (selected, selected_count) = match &report.selected_attack.attack {
            SelectedMeleeView::Natural { attack } => (
                StrikeSourceAttribution::Natural {
                    species: attack.source.clone(),
                },
                attack.strike_count,
            ),
            SelectedMeleeView::Weapon { .. } => (
                StrikeSourceAttribution::Weapon {
                    source: report.selected_attack.source.clone(),
                    socket: report.selected_attack.socket.ok_or(STALE)?,
                },
                1,
            ),
        };
        let mut sources = vec![
            Source {
                choice: StrikeSourceChoice {
                    source: StrikeSourceSelector::Natural {},
                    label: "Natural strikes".into(),
                },
                resolved: natural,
                count: report.natural_attack.strike_count,
            },
            Source {
                choice: StrikeSourceChoice {
                    source: StrikeSourceSelector::SelectedMelee {},
                    label: "Selected melee".into(),
                },
                resolved: selected,
                count: selected_count,
            },
        ];
        for (summary, ability) in report.ability_strikes.iter().zip(&report.stats.abilities) {
            sources.push(Source {
                choice: StrikeSourceChoice {
                    source: StrikeSourceSelector::Ability {
                        id: summary.ability.clone(),
                    },
                    label: format!("{} ({})", ability.name, ability.id),
                },
                resolved: match summary.source {
                    StrikeSourceInspectionView::NaturalMelee {
                        additional_riders, ..
                    } => StrikeSourceAttribution::NaturalMelee {
                        species: report.natural_attack.source.clone(),
                        ability: ability.content(),
                        additional_riders,
                        // The first validated page supplies these bounded source counts.
                        natural_riders: Vec::new(),
                    },
                    _ => StrikeSourceAttribution::Ability {
                        ability: ability.content(),
                    },
                },
                count: match summary.source {
                    StrikeSourceInspectionView::Authored { strike_count }
                    | StrikeSourceInspectionView::NaturalMelee { strike_count, .. } => strike_count,
                    StrikeSourceInspectionView::SelectedMelee {} => 0,
                },
            });
        }
        self.choices = sources.iter().map(|source| source.choice.clone()).collect();
        self.target = Some(Target {
            actor: report.actor,
            revision: report.revision,
            context: context.clone(),
            sources,
        });
        Ok(())
    }
    pub fn sources(&self) -> &[StrikeSourceChoice] {
        &self.choices
    }
    fn current(&mut self, context: &ReplyContext) -> Result<(), String> {
        if self
            .target
            .as_ref()
            .is_none_or(|target| &target.context != context)
        {
            self.clear();
            return Err(STALE.into());
        }
        if self.pending.is_some() {
            return Err("Waiting for the strike detail reply.".into());
        }
        Ok(())
    }
    pub fn select(
        &mut self,
        source: StrikeSourceSelector,
        current: &ReplyContext,
    ) -> Result<Command, String> {
        self.current(current)?;
        let source = self
            .target
            .as_ref()
            .unwrap()
            .sources
            .iter()
            .find(|entry| entry.choice.source == source)
            .cloned()
            .ok_or("That strike source is unavailable.")?;
        self.active = Some(Active {
            source,
            handle: None,
            current: 0,
            cursors: vec![],
            next: None,
            rendered: VecDeque::new(),
            structure: Structure::default(),
        });
        self.query(None, 0)
    }
    fn query(&mut self, cursor: Option<StrikePageCursor>, index: usize) -> Result<Command, String> {
        let target = self.target.as_ref().ok_or(STALE)?;
        let active = self
            .active
            .as_ref()
            .ok_or("Select a strike source first.")?;
        let command = Command::InspectStrikeSource {
            expected_revision: target.context.revision,
            actor: target.actor,
            target_revision: target.revision,
            source: active.source.choice.source.clone(),
            cursor: cursor.clone(),
        };
        self.pending = Some(Pending {
            cursor,
            index,
            request_id: None,
        });
        Ok(command)
    }
    pub fn next(&mut self, current: &ReplyContext) -> Result<Command, String> {
        self.current(current)?;
        let active = self
            .active
            .as_ref()
            .ok_or("Select a strike source first.")?;
        let cursor = active.next.clone().ok_or("End of strike source.")?;
        let index = active.current + 1;
        self.query(Some(cursor), index)
    }
    pub fn previous(&mut self, current: &ReplyContext) -> Result<Command, String> {
        self.current(current)?;
        let active = self
            .active
            .as_ref()
            .ok_or("Select a strike source first.")?;
        let index = active.current.checked_sub(1).ok_or("First strike page.")?;
        let cursor = active.cursors.get(index).cloned().ok_or(STALE)?;
        self.query(Some(cursor), index)
    }
    pub fn bind_request(&mut self, id: &str) {
        if let Some(pending) = &mut self.pending {
            pending.request_id = Some(id.into());
        }
    }
    pub fn accept(
        &mut self,
        context: &ReplyContext,
        request_id: &str,
        page: &CreatureStrikePage,
    ) -> Result<(), String> {
        let result = self.accept_inner(context, request_id, page);
        if result.is_err() {
            self.clear();
        }
        result
    }
    fn accept_inner(
        &mut self,
        context: &ReplyContext,
        request_id: &str,
        page: &CreatureStrikePage,
    ) -> Result<(), String> {
        let pending = self.pending.as_ref().ok_or("No pending strike query.")?;
        let target = self.target.as_ref().ok_or(STALE)?;
        let active = self.active.as_ref().ok_or(STALE)?;
        page.validate().map_err(|error| error.to_string())?;
        let first = if active.source.count == 0 {
            StrikePagePosition::SelectedMelee {}
        } else if matches!(
            active.source.resolved,
            StrikeSourceAttribution::Weapon { .. }
        ) {
            StrikePagePosition::Weapon {}
        } else {
            StrikePagePosition::StrikeHeader { strike: 0 }
        };
        let same_source = match (&active.source.resolved, &page.resolved) {
            (
                StrikeSourceAttribution::NaturalMelee {
                    species: expected_species,
                    ability: expected_ability,
                    additional_riders: expected_added,
                    natural_riders: expected_natural,
                },
                StrikeSourceAttribution::NaturalMelee {
                    species,
                    ability,
                    additional_riders,
                    natural_riders,
                },
            ) => {
                species == expected_species
                    && ability == expected_ability
                    && additional_riders == expected_added
                    && (active.handle.is_none() || natural_riders == expected_natural)
            }
            (expected, actual) => expected == actual,
        };
        if &target.context != context
            || pending
                .request_id
                .as_ref()
                .is_some_and(|id| id != request_id)
            || page.actor != target.actor
            || page.target_revision != target.revision
            || page.source != active.source.choice.source
            || !same_source
            || page.strike_count != active.source.count
            || page.start
                != pending
                    .cursor
                    .as_ref()
                    .map_or(first, |cursor| cursor.position.clone())
            || pending
                .cursor
                .as_ref()
                .is_some_and(|cursor| cursor.inspection != page.inspection)
            || active
                .handle
                .as_ref()
                .is_some_and(|handle| handle != &page.inspection)
            || pending.index > active.cursors.len()
            || active.cursors.len() >= MAX_STRIKE_SOURCE_ENTRIES
                && pending.index == active.cursors.len()
        {
            return Err(STALE.into());
        }
        let mut structure = active.structure.clone();
        structure.check(page)?;
        let cursor = StrikePageCursor {
            inspection: page.inspection.clone(),
            position: page.start.clone(),
        };
        if active
            .cursors
            .get(pending.index)
            .is_some_and(|expected| expected != &cursor)
            || active
                .cursors
                .get(pending.index + 1)
                .is_some_and(|expected| page.next.as_ref() != Some(expected))
        {
            return Err(STALE.into());
        }
        let active = self.active.as_mut().unwrap();
        // Evict before formatting so staging the candidate never retains a third page.
        active.rendered.retain(|cached| cached.cursor != cursor);
        if active.rendered.len() == 2 {
            active.rendered.pop_front();
        }
        let rows = crate::inspection::page_lines(page).map_err(|error| error.to_string())?;
        if pending.index == active.cursors.len() {
            active.cursors.push(cursor.clone());
        }
        active.current = pending.index;
        active.source.resolved = page.resolved.clone();
        active.handle = Some(page.inspection.clone());
        active.next = page.next.clone();
        active.structure = structure;
        active.rendered.push_back(Rendered { cursor, rows });
        self.pending = None;
        Ok(())
    }
    pub fn lines(&self) -> Option<&[String]> {
        self.active
            .as_ref()?
            .rendered
            .back()
            .map(|page| page.rows.as_slice())
    }
    pub fn retained_pages(&self) -> usize {
        self.active
            .as_ref()
            .map_or(0, |active| active.rendered.len())
    }
    pub fn cursor_count(&self) -> usize {
        self.active
            .as_ref()
            .map_or(0, |active| active.cursors.len())
    }
}
impl Structure {
    fn strike_after(&self, strike: u16, count: u16) -> Option<StrikePagePosition> {
        (strike + 1 < count).then_some(StrikePagePosition::StrikeHeader { strike: strike + 1 })
    }
    fn after_primary(
        &self,
        strike: u16,
        section: u8,
        count: u16,
    ) -> Result<Option<StrikePagePosition>, String> {
        let lengths = self.strikes.get(&strike).ok_or(STALE)?;
        Ok(if section < 1 && lengths[0] > 0 {
            Some(StrikePagePosition::SuccessEffect { strike, index: 0 })
        } else if section < 2 && lengths[1] > 0 {
            Some(StrikePagePosition::FailureEffect { strike, index: 0 })
        } else if section < 3 && lengths[2] > 0 {
            Some(StrikePagePosition::StrikeDescriptor { strike, index: 0 })
        } else if lengths[3] > 0 {
            Some(StrikePagePosition::RiderHeader { strike, rider: 0 })
        } else {
            self.strike_after(strike, count)
        })
    }
    fn after_rider(
        &self,
        strike: u16,
        rider: u16,
        section: u8,
        count: u16,
    ) -> Result<Option<StrikePagePosition>, String> {
        let lengths = &self.riders.get(&(strike, rider)).ok_or(STALE)?.lengths;
        Ok(if section < 1 && lengths[0] > 0 {
            Some(StrikePagePosition::RiderEffect {
                strike,
                rider,
                index: 0,
            })
        } else if section < 2 && lengths[1] > 0 {
            Some(StrikePagePosition::RiderDescriptor {
                strike,
                rider,
                index: 0,
            })
        } else if rider + 1 < self.strikes.get(&strike).ok_or(STALE)?[3] {
            Some(StrikePagePosition::RiderHeader {
                strike,
                rider: rider + 1,
            })
        } else {
            self.strike_after(strike, count)
        })
    }
    fn check(&mut self, page: &CreatureStrikePage) -> Result<(), String> {
        let mut expected = Some(page.start.clone());
        for entry in &page.entries {
            if expected.as_ref() != Some(&entry.position()) {
                return Err("Strike page omitted or repeated an authored entry.".into());
            }
            expected = match *entry {
                StrikePageEntry::StrikeHeader {
                    strike,
                    offset,
                    ref check,
                    success_count,
                    failure_count,
                    descriptor_count,
                    rider_count,
                    ..
                } => {
                    if strike > 0
                        && self
                            .offsets
                            .get(&(strike - 1))
                            .is_none_or(|previous| offset < *previous)
                    {
                        return Err(STALE.into());
                    }
                    if self
                        .offsets
                        .insert(strike, offset)
                        .is_some_and(|old| old != offset)
                    {
                        return Err(STALE.into());
                    }
                    if self
                        .primary_checked
                        .insert(strike, check.is_some())
                        .is_some_and(|old| old != check.is_some())
                    {
                        return Err(STALE.into());
                    }
                    let lengths = [success_count, failure_count, descriptor_count, rider_count];
                    if self
                        .strikes
                        .insert(strike, lengths)
                        .is_some_and(|old| old != lengths)
                    {
                        return Err(STALE.into());
                    }
                    self.after_primary(strike, 0, page.strike_count)?
                }
                StrikePageEntry::RiderHeader {
                    strike,
                    rider,
                    ref trigger,
                    condition,
                    effect_count,
                    descriptor_count,
                    ..
                } => {
                    if rider >= self.strikes.get(&strike).ok_or(STALE)?[3] {
                        return Err(STALE.into());
                    }
                    if *trigger == RiderTrigger::Failure
                        && !self.primary_checked.get(&strike).copied().ok_or(STALE)?
                    {
                        return Err(
                            "An unchecked primary cannot have a failure-triggered rider.".into(),
                        );
                    }
                    let shape = RiderShape {
                        trigger: *trigger,
                        condition,
                        lengths: [effect_count, descriptor_count],
                    };
                    if self
                        .riders
                        .get(&(strike, rider))
                        .is_some_and(|old| old != &shape)
                    {
                        return Err(STALE.into());
                    }
                    self.riders.insert((strike, rider), shape);
                    self.after_rider(strike, rider, 0, page.strike_count)?
                }
                StrikePageEntry::SuccessEffect { strike, index, .. } => {
                    if index >= self.strikes.get(&strike).ok_or(STALE)?[0] {
                        return Err(STALE.into());
                    }
                    if index + 1 < self.strikes[&strike][0] {
                        Some(StrikePagePosition::SuccessEffect {
                            strike,
                            index: index + 1,
                        })
                    } else {
                        self.after_primary(strike, 1, page.strike_count)?
                    }
                }
                StrikePageEntry::FailureEffect { strike, index, .. } => {
                    if index >= self.strikes.get(&strike).ok_or(STALE)?[1] {
                        return Err(STALE.into());
                    }
                    if index + 1 < self.strikes[&strike][1] {
                        Some(StrikePagePosition::FailureEffect {
                            strike,
                            index: index + 1,
                        })
                    } else {
                        self.after_primary(strike, 2, page.strike_count)?
                    }
                }
                StrikePageEntry::StrikeDescriptor { strike, index, .. } => {
                    if index >= self.strikes.get(&strike).ok_or(STALE)?[2] {
                        return Err(STALE.into());
                    }
                    if index + 1 < self.strikes[&strike][2] {
                        Some(StrikePagePosition::StrikeDescriptor {
                            strike,
                            index: index + 1,
                        })
                    } else {
                        self.after_primary(strike, 3, page.strike_count)?
                    }
                }
                StrikePageEntry::RiderEffect {
                    strike,
                    rider,
                    index,
                    ..
                } => {
                    if index >= self.riders.get(&(strike, rider)).ok_or(STALE)?.lengths[0] {
                        return Err(STALE.into());
                    }
                    if index + 1 < self.riders[&(strike, rider)].lengths[0] {
                        Some(StrikePagePosition::RiderEffect {
                            strike,
                            rider,
                            index: index + 1,
                        })
                    } else {
                        self.after_rider(strike, rider, 1, page.strike_count)?
                    }
                }
                StrikePageEntry::RiderDescriptor {
                    strike,
                    rider,
                    index,
                    ..
                } => {
                    if index >= self.riders.get(&(strike, rider)).ok_or(STALE)?.lengths[1] {
                        return Err(STALE.into());
                    }
                    if index + 1 < self.riders[&(strike, rider)].lengths[1] {
                        Some(StrikePagePosition::RiderDescriptor {
                            strike,
                            rider,
                            index: index + 1,
                        })
                    } else {
                        self.after_rider(strike, rider, 2, page.strike_count)?
                    }
                }
                StrikePageEntry::Weapon { .. } | StrikePageEntry::SelectedMelee {} => None,
            };
            let effects = self
                .strikes
                .values()
                .map(|counts| usize::from(counts[0]) + usize::from(counts[1]))
                .sum::<usize>()
                + self
                    .riders
                    .values()
                    .map(|shape| usize::from(shape.lengths[0]))
                    .sum::<usize>();
            let descriptors = self
                .strikes
                .values()
                .map(|counts| usize::from(counts[2]))
                .sum::<usize>()
                + self
                    .riders
                    .values()
                    .map(|shape| usize::from(shape.lengths[1]))
                    .sum::<usize>();
            if effects > 256 || descriptors > 256 {
                return Err("Strike source list totals exceed their bounds.".into());
            }
        }
        if page.next.as_ref().map(|cursor| &cursor.position) != expected.as_ref() {
            return Err("Strike source continuation is incomplete.".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod ordered_strikes_detail_state_tests {
    use super::*;
    fn context() -> ReplyContext {
        ReplyContext {
            input: InputContext {
                stream: StreamContext {
                    stream: StreamId("private".into()),
                    epoch: 0,
                },
                readiness_revision: 0,
            },
            actor: ActorId(1),
            branch: BranchId("original".into()),
            cursor: StreamCursor {
                sequence: 0,
                tick: 0,
            },
            revision: 0,
        }
    }
    fn pages() -> Vec<CreatureStrikePage> {
        let mut entries = vec![StrikePageEntry::StrikeHeader {
            strike: 0,
            offset: 0,
            unlock: None,
            check: None,
            success_count: 0,
            failure_count: 0,
            descriptor_count: 32,
            rider_count: 0,
        }];
        entries.extend((0..32).map(|index| StrikePageEntry::StrikeDescriptor {
            strike: 0,
            index,
            descriptor: crate::test_support::content(&format!("d_{index}")),
        }));
        entries
            .chunks(16)
            .enumerate()
            .map(|(index, chunk)| CreatureStrikePage {
                inspection: StrikeInspectionId("one-handle".into()),
                actor: ActorId(2),
                target_revision: 5,
                source: StrikeSourceSelector::Natural {},
                resolved: StrikeSourceAttribution::Natural {
                    species: crate::test_support::content("human"),
                },
                strike_count: 1,
                start: chunk[0].position(),
                entries: chunk.to_vec(),
                next: entries.get((index + 1) * 16).map(|entry| StrikePageCursor {
                    inspection: StrikeInspectionId("one-handle".into()),
                    position: entry.position(),
                }),
            })
            .collect()
    }
    fn inspector() -> InspectionPages {
        let mut state = InspectionPages::default();
        state
            .set_report(&crate::inspection::tests::report(), &context())
            .unwrap();
        state
    }
    #[test]
    fn natural_melee_navigation_pins_species_rider_boundaries_across_pages() {
        let mut value = serde_json::to_value(crate::inspection::tests::report()).unwrap();
        value["stats"]["abilities"] = serde_json::json!([{"id":"natural_technique","name":"Natural technique",
            "cost":null,"reach":{"kind":"melee"},"target_kind":"actor","basic_attack":false,"weapon_selection":false,
            "cooldown_remaining":"0","supply_costs":[]}]);
        value["ability_strikes"] = serde_json::json!([{"ability":"natural_technique",
            "source":{"kind":"natural_melee","strike_count":1,"additional_riders":0}}]);
        let report: CreatureInspectionView = serde_json::from_value(value).unwrap();
        let mut state = InspectionPages::default();
        state.set_report(&report, &context()).unwrap();
        let source = StrikeSourceSelector::Ability {
            id: "natural_technique".into(),
        };
        state.select(source.clone(), &context()).unwrap();
        let mut pages = pages();
        for page in &mut pages {
            page.source = source.clone();
            page.resolved = serde_json::from_value(serde_json::json!({"kind":"natural_melee",
                "species":{"id":"human","name":"Human"},
                "ability":{"id":"natural_technique","name":"Natural technique"},
                "additional_riders":0,"natural_riders":[0]}))
            .unwrap();
        }
        state.accept(&context(), "first", &pages[0]).unwrap();
        state.next(&context()).unwrap();
        let mut changed = serde_json::to_value(&pages[1]).unwrap();
        changed["resolved"]["natural_riders"] = serde_json::json!([1]);
        let changed: CreatureStrikePage = serde_json::from_value(changed).unwrap();
        assert!(state.accept(&context(), "changed", &changed).is_err());
        assert!(state.lines().is_none());
        state.set_report(&report, &context()).unwrap();
        state.select(source, &context()).unwrap();
        for (index, page) in pages.iter().enumerate() {
            if index > 0 {
                state.next(&context()).unwrap();
            }
            state.accept(&context(), "page", page).unwrap();
        }
        assert!(state
            .lines()
            .unwrap()
            .join("\n")
            .contains("Natural source: Human"));
    }

    #[test]
    fn ordered_strikes_detail_traverses_complete_source_retains_exactly_two_pages_and_revisits_zero_with_handle(
    ) {
        let mut state = inspector();
        let query = state
            .select(StrikeSourceSelector::Natural {}, &context())
            .expect("main summary opens a typed source query");
        assert!(matches!(
            query,
            Command::InspectStrikeSource {
                expected_revision: 0,
                actor: ActorId(2),
                target_revision: 5,
                cursor: None,
                ..
            }
        ));
        let pages = pages();
        let mut descriptors = Vec::new();
        for (index, page) in pages.iter().enumerate() {
            page.validate().unwrap();
            state.bind_request("current");
            state.accept(&context(), "current", page).unwrap();
            descriptors.extend(page.entries.iter().filter_map(|entry| {
                if let StrikePageEntry::StrikeDescriptor { index, .. } = entry {
                    Some(*index)
                } else {
                    None
                }
            }));
            assert!(state.lines().unwrap()[0].starts_with("Strike source:"));
            assert_eq!(state.retained_pages(), (index + 1).min(2));
            assert_eq!(state.cursor_count(), index + 1);
            if let Some(cursor) = &page.next {
                assert!(
                    matches!(state.next(&context()).unwrap(), Command::InspectStrikeSource { cursor: Some(ref requested), .. } if requested == cursor)
                );
            }
        }
        assert_eq!(descriptors, (0..32).collect::<Vec<_>>());
        assert!(state.next(&context()).is_err());
        for index in (0..2).rev() {
            let command = state.previous(&context()).unwrap();
            assert!(
                matches!(command, Command::InspectStrikeSource { cursor: Some(ref cursor), .. } if cursor.inspection == pages[0].inspection && cursor.position == pages[index].start)
            );
            state.bind_request("back");
            state.accept(&context(), "back", &pages[index]).unwrap();
            assert_eq!(state.retained_pages(), 2);
        }
        assert!(state.previous(&context()).is_err());
    }
    #[test]
    fn ordered_strikes_detail_refuses_unsolicited_wrong_source_target_revision_context_and_request_without_installing(
    ) {
        let page = pages().remove(0);
        let mut valid = inspector();
        valid
            .select(StrikeSourceSelector::Natural {}, &context())
            .unwrap();
        valid.bind_request("valid");
        valid
            .accept(&context(), "valid", &page)
            .expect("valid first page must install before testing rejection");
        let mut unopened = inspector();
        assert!(unopened.accept(&context(), "unsolicited", &page).is_err());
        for fault in 0..6 {
            let mut state = inspector();
            state
                .select(StrikeSourceSelector::Natural {}, &context())
                .unwrap();
            state.bind_request("pending");
            let mut invalid = page.clone();
            let mut reply = context();
            let mut id = "pending";
            match fault {
                0 => invalid.source = StrikeSourceSelector::SelectedMelee {},
                1 => invalid.actor = ActorId(3),
                2 => invalid.target_revision += 1,
                3 => reply.input.stream.epoch += 1,
                4 => reply.revision += 1,
                _ => id = "unsolicited",
            }
            assert!(state.accept(&reply, id, &invalid).is_err(), "fault {fault}");
            assert_eq!(state.retained_pages(), 0);
            assert!(state.lines().is_none());
        }
    }
    #[test]
    fn ordered_strikes_detail_rejects_skipped_list_entry_and_requires_explicit_reopen_after_clear()
    {
        let mut state = inspector();
        state
            .select(StrikeSourceSelector::Natural {}, &context())
            .unwrap();
        let mut page = pages().remove(0);
        state
            .accept(&context(), "valid", &page)
            .expect("valid page must install before malformed-list rejection");
        state
            .select(StrikeSourceSelector::Natural {}, &context())
            .unwrap();
        page.entries.remove(2);
        assert!(state.accept(&context(), "pending", &page).is_err());
        state.clear();
        assert!(state.next(&context()).is_err());
        assert!(state.previous(&context()).is_err());
        assert!(state
            .select(StrikeSourceSelector::Natural {}, &context())
            .is_err());
        assert_eq!(state.retained_pages(), 0);
    }
    #[test]
    fn ordered_strikes_detail_rejects_aggregate_257_effects_and_descriptors_across_pages() {
        for descriptors in [false, true] {
            let mut state = InspectionPages::default();
            let mut report = crate::inspection::tests::report();
            report.natural_attack.strike_count = 3;
            state.set_report(&report, &context()).unwrap();
            state
                .select(StrikeSourceSelector::Natural {}, &context())
                .unwrap();
            let mut entries = Vec::new();
            for (strike, count) in [(0, 128), (1, 128), (2, 1)] {
                entries.push(StrikePageEntry::StrikeHeader {
                    strike,
                    offset: u64::from(strike),
                    unlock: None,
                    check: None,
                    success_count: if descriptors { 0 } else { count },
                    failure_count: 0,
                    descriptor_count: if descriptors { count } else { 0 },
                    rider_count: 0,
                });
                for index in 0..count {
                    entries.push(if descriptors {
                        StrikePageEntry::StrikeDescriptor {
                            strike,
                            index,
                            descriptor: crate::test_support::content("mind_affecting"),
                        }
                    } else {
                        StrikePageEntry::SuccessEffect {
                            strike,
                            index,
                            row: CombatTraceStep {
                                id: "damage".into(),
                                name: "Damage".into(),
                                fields: vec![],
                            },
                        }
                    });
                }
            }
            let mut rejected = false;
            for (number, chunk) in entries.chunks(16).enumerate() {
                let mut page = pages().remove(0);
                page.strike_count = 3;
                page.start = chunk[0].position();
                page.entries = chunk.to_vec();
                page.next = entries
                    .get((number + 1) * 16)
                    .map(|entry| StrikePageCursor {
                        inspection: page.inspection.clone(),
                        position: entry.position(),
                    });
                state.bind_request("aggregate");
                let accepted = state.accept(&context(), "aggregate", &page);
                if number < 16 {
                    accepted.expect("the first 256 aggregate entries must remain inspectable");
                } else if accepted.is_err() {
                    rejected = true;
                    break;
                }
                assert!(state.retained_pages() <= 2);
                if page.next.is_some() {
                    state.next(&context()).unwrap();
                }
            }
            assert!(
                rejected,
                "257 aggregate authored entries cannot be installed across pages"
            );
            assert_eq!(
                state.retained_pages(),
                0,
                "invalid continuation clears retained details"
            );
        }
    }

    #[test]
    fn ordered_strikes_detail_traverses_all_656_entries_with_bounded_rendered_retention() {
        let mut state = InspectionPages::default();
        let mut report = crate::inspection::tests::report();
        report.natural_attack.strike_count = 16;
        state.set_report(&report, &context()).unwrap();
        state
            .select(StrikeSourceSelector::Natural {}, &context())
            .unwrap();
        let mut entries = Vec::new();
        for strike in 0..16 {
            entries.push(StrikePageEntry::StrikeHeader {
                strike,
                offset: u64::from(strike),
                unlock: None,
                check: None,
                success_count: 0,
                failure_count: 0,
                descriptor_count: 16,
                rider_count: 8,
            });
            for index in 0..16 {
                entries.push(StrikePageEntry::StrikeDescriptor {
                    strike,
                    index,
                    descriptor: crate::test_support::content("mind_affecting"),
                });
            }
            for rider in 0..8 {
                entries.push(StrikePageEntry::RiderHeader {
                    strike,
                    rider,
                    trigger: RiderTrigger::Success,
                    condition: None,
                    check: None,
                    effect_count: 2,
                    descriptor_count: 0,
                });
                for index in 0..2 {
                    entries.push(StrikePageEntry::RiderEffect {
                        strike,
                        rider,
                        index,
                        row: CombatTraceStep {
                            id: "damage".into(),
                            name: "Damage".into(),
                            fields: vec![],
                        },
                    });
                }
            }
        }
        assert_eq!(entries.len(), MAX_STRIKE_SOURCE_ENTRIES);
        let mut visited = 0;
        for (number, chunk) in entries.chunks(16).enumerate() {
            let mut page = pages().remove(0);
            page.strike_count = 16;
            page.start = chunk[0].position();
            page.entries = chunk.to_vec();
            page.next = entries
                .get((number + 1) * 16)
                .map(|entry| StrikePageCursor {
                    inspection: page.inspection.clone(),
                    position: entry.position(),
                });
            state.bind_request("maximum");
            state.accept(&context(), "maximum", &page).unwrap();
            visited += page.entries.len();
            assert_eq!(state.retained_pages(), (number + 1).min(2));
            assert!(state.cursor_count() <= MAX_STRIKE_SOURCE_ENTRIES);
            if page.next.is_some() {
                state.next(&context()).unwrap();
            }
        }
        assert_eq!(visited, MAX_STRIKE_SOURCE_ENTRIES);
        assert_eq!(state.cursor_count(), 41);
        assert!(state.next(&context()).is_err());
    }
    #[test]
    fn ordered_strikes_detail_refuses_changed_handle_or_cursor_before_installing_continuation() {
        for wrong_handle in [false, true] {
            let mut state = inspector();
            state
                .select(StrikeSourceSelector::Natural {}, &context())
                .unwrap();
            state.accept(&context(), "first", &pages()[0]).unwrap();
            state.next(&context()).unwrap();
            state.bind_request("continuation");
            let mut page = pages()[1].clone();
            if wrong_handle {
                page.inspection = StrikeInspectionId("replaced-handle".into());
                if let Some(cursor) = &mut page.next {
                    cursor.inspection = page.inspection.clone();
                }
            } else {
                page.start = StrikePagePosition::StrikeDescriptor {
                    strike: 0,
                    index: 16,
                };
                page.entries.remove(0);
            }
            assert!(state.accept(&context(), "continuation", &page).is_err());
            assert_eq!(state.retained_pages(), 0);
            assert!(state.next(&context()).is_err());
        }
    }

    #[test]
    fn ordered_strikes_detail_rejects_failure_rider_after_prior_page_unchecked_primary() {
        let mut state = inspector();
        state
            .select(StrikeSourceSelector::Natural {}, &context())
            .unwrap();
        let mut first = pages().remove(0);
        first.entries = vec![StrikePageEntry::StrikeHeader {
            strike: 0,
            offset: 0,
            unlock: None,
            check: None,
            success_count: 0,
            failure_count: 0,
            descriptor_count: 0,
            rider_count: 1,
        }];
        first.next = Some(StrikePageCursor {
            inspection: first.inspection.clone(),
            position: StrikePagePosition::RiderHeader {
                strike: 0,
                rider: 0,
            },
        });
        first.validate().unwrap();
        state.accept(&context(), "first", &first).unwrap();
        state.next(&context()).unwrap();
        let mut second = first;
        second.start = StrikePagePosition::RiderHeader {
            strike: 0,
            rider: 0,
        };
        second.entries = vec![StrikePageEntry::RiderHeader {
            strike: 0,
            rider: 0,
            trigger: RiderTrigger::Failure,
            condition: None,
            check: None,
            effect_count: 0,
            descriptor_count: 0,
        }];
        second.next = None;
        second.validate().unwrap();
        assert!(
            state.accept(&context(), "failure", &second).is_err(),
            "a previous-page unchecked primary cannot have a failure-triggered rider"
        );
        assert_eq!(state.retained_pages(), 0);
    }

    #[test]
    fn sequence_rider_condition_is_pinned_after_rendered_page_eviction() {
        for changed in [
            None,
            Some(InspectionRiderCondition::SequenceSuccessfulHits { minimum: 3 }),
        ] {
            let mut state = inspector();
            state
                .select(StrikeSourceSelector::Natural {}, &context())
                .unwrap();
            let mut pages = pages();
            if let StrikePageEntry::StrikeHeader { rider_count, .. } = &mut pages[0].entries[0] {
                *rider_count = 1;
            }
            pages[2].entries.push(StrikePageEntry::RiderHeader {
                strike: 0,
                rider: 0,
                trigger: RiderTrigger::Success,
                condition: Some(InspectionRiderCondition::SequenceSuccessfulHits { minimum: 2 }),
                check: None,
                effect_count: 48,
                descriptor_count: 0,
            });
            let effects: Vec<_> = (0..48)
                .map(|index| StrikePageEntry::RiderEffect {
                    strike: 0,
                    rider: 0,
                    index,
                    row: CombatTraceStep {
                        id: "heal".into(),
                        name: "Heal".into(),
                        fields: vec![],
                    },
                })
                .collect();
            pages[2].next = Some(StrikePageCursor {
                inspection: pages[2].inspection.clone(),
                position: effects[0].position(),
            });
            for (number, chunk) in effects.chunks(16).enumerate() {
                let mut page = pages[0].clone();
                page.start = chunk[0].position();
                page.entries = chunk.to_vec();
                page.next = effects
                    .get((number + 1) * 16)
                    .map(|entry| StrikePageCursor {
                        inspection: page.inspection.clone(),
                        position: entry.position(),
                    });
                pages.push(page);
            }
            for (number, page) in pages.iter().enumerate() {
                if number > 0 {
                    state.next(&context()).unwrap();
                }
                state.accept(&context(), "page", page).unwrap();
            }
            assert_eq!(state.retained_pages(), 2);
            // The header's rendered page has been evicted, but its source shape remains.
            for index in (2..5).rev() {
                state.previous(&context()).unwrap();
                state
                    .accept(&context(), "unchanged", &pages[index])
                    .unwrap();
            }
            assert!(state
                .lines()
                .unwrap()
                .join("\n")
                .contains("Condition: at least 2"));
            state.next(&context()).unwrap();
            state.accept(&context(), "page", &pages[3]).unwrap();
            state.previous(&context()).unwrap();
            let mut page = pages[2].clone();
            if let Some(StrikePageEntry::RiderHeader { condition, .. }) = page.entries.last_mut() {
                *condition = changed;
            }
            assert!(state.accept(&context(), "changed", &page).is_err());
            assert_eq!(state.retained_pages(), 0);
        }
    }
}
