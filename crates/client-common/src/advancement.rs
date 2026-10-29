//! Owner-only, stateless advancement navigation. This is a presentation reducer,
//! not a rules engine: every price, refund and resulting build comes from a page.
use std::collections::VecDeque;
use tor_protocol::*;

const STALE: &str = "Advancement preview is stale. Open advancement and explicitly preview again.";
pub const HELP: &str = "overview | ledger | balances | skills | talents | expertises | intellect-expertises | next | previous\nsource racial | source class <class ID>\nintellect-expertise <threshold> <expertise ID>\npurchase <HD> <pool ID> <skill|talent|expertise> <ID> [form <form ID>]\nremove <HD> <purchase number> | replace <HD> <purchase number> <pool ID> <skill|talent|expertise> <ID> [form <form ID>]\ndetails <summary|removed|refunds|balances|skills|talents|expertises|conditions>\nsupplies | confirm [using carried|<name or #item>] | resume | cancel\nHD and purchase numbers start at 1. Choices preview first; only confirm commits.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdvancementInput {
    Overview,
    Ledger,
    Balances,
    IntellectExpertises,
    Catalog(AdvancementKind),
    Preview(AdvancementProposal),
    Details(AdvancementPreviewSection),
    Next,
    Previous,
    Confirm(Option<String>),
    Supplies,
    Resume,
    Cancel,
}

/// A human-facing grammar shared by Text, native ASCII and headless. Identifiers
/// are copied exactly. One-based labels never leak into zero-based wire operands.
pub fn parse(text: &str) -> Result<AdvancementInput, String> {
    let text = text.trim();
    if text == "confirm" {
        return Ok(AdvancementInput::Confirm(None));
    }
    if let Some(supply) = text
        .strip_prefix("confirm using ")
        .filter(|v| !v.trim().is_empty())
    {
        return Ok(AdvancementInput::Confirm(Some(supply.trim().into())));
    }
    let fields: Vec<_> = text.split_whitespace().collect();
    let purchase_index = |text: &str| -> Result<u16, String> {
        text.parse::<u16>()
            .ok()
            .filter(|n| (1..=256).contains(n))
            .map(|n| n - 1)
            .ok_or_else(|| "Purchase numbers must be 1 through 256.".into())
    };
    let owner_index = |text: &str| -> Result<u64, String> {
        text.parse::<u64>()
            .ok()
            .and_then(|ordinal| ordinal.checked_sub(1))
            .ok_or_else(|| "HD must be a positive 64-bit ordinal.".into())
    };
    let choice = |kind: &str, id: &str| -> Result<PurchaseChoice, String> {
        if !valid_content_id(id) {
            return Err("Use the exact ID disclosed in the catalog.".into());
        }
        match kind {
            "skill" => Ok(PurchaseChoice::SkillRank { skill: id.into() }),
            "talent" => Ok(PurchaseChoice::Talent { talent: id.into() }),
            "expertise" => Ok(PurchaseChoice::Expertise {
                expertise: id.into(),
            }),
            _ => Err("Choose skill, talent or expertise.".into()),
        }
    };
    let form_choice = |talent: &str, form: &str| -> Result<PurchaseChoice, String> {
        if !valid_content_id(talent) || !valid_content_id(form) {
            return Err("Use the exact talent and form IDs disclosed in the catalog.".into());
        }
        Ok(PurchaseChoice::TalentForm {
            talent: talent.into(),
            form: form.into(),
        })
    };
    Ok(match fields.as_slice() {
        [] | ["overview"] => AdvancementInput::Overview,
        ["ledger"] => AdvancementInput::Ledger,
        ["balances"] => AdvancementInput::Balances,
        ["skills"] => AdvancementInput::Catalog(AdvancementKind::SkillRank),
        ["talents"] => AdvancementInput::Catalog(AdvancementKind::Talent),
        ["intellect-expertises"] => AdvancementInput::IntellectExpertises,
        ["intellect-expertise", threshold, expertise] if valid_content_id(expertise) => {
            let threshold = threshold
                .parse::<u16>()
                .ok()
                .filter(|n| (1..=1000).contains(n))
                .ok_or("Intellect thresholds must be 1 through 1000.")?;
            AdvancementInput::Preview(AdvancementProposal::IntellectExpertise {
                choice: IntellectExpertiseOperation {
                    threshold,
                    expertise: (*expertise).into(),
                },
            })
        }
        ["expertises"] => AdvancementInput::Catalog(AdvancementKind::Expertise),
        ["next"] => AdvancementInput::Next,
        ["previous"] => AdvancementInput::Previous,
        ["supplies"] => AdvancementInput::Supplies,
        ["resume"] => AdvancementInput::Resume,
        ["cancel"] => AdvancementInput::Cancel,
        ["purchase", owner, pool, "talent", talent, "form", form] => {
            AdvancementInput::Preview(AdvancementProposal::Purchase {
                purchase: PurchaseOperation {
                    owner: owner_index(owner)?,
                    pool: (*pool).into(),
                    selection: form_choice(talent, form)?,
                },
            })
        }
        ["source", "racial"] => AdvancementInput::Preview(AdvancementProposal::GrowthSource {
            source: GrowthSource::Racial {},
        }),
        ["source", "class", class] if valid_content_id(class) => {
            AdvancementInput::Preview(AdvancementProposal::GrowthSource {
                source: GrowthSource::Class {
                    class: (*class).into(),
                },
            })
        }
        ["purchase", owner, pool, kind, id] => {
            AdvancementInput::Preview(AdvancementProposal::Purchase {
                purchase: PurchaseOperation {
                    owner: owner_index(owner)?,
                    pool: (*pool).into(),
                    selection: choice(kind, id)?,
                },
            })
        }
        ["remove", owner, index] => AdvancementInput::Preview(AdvancementProposal::Replacement {
            replacement: ReplacementOperation {
                source: ReplacementSource {
                    owner: owner_index(owner)?,
                    index: purchase_index(index)?,
                },
                replacement: None,
            },
        }),
        ["replace", owner, index, pool, "talent", talent, "form", form] => {
            AdvancementInput::Preview(AdvancementProposal::Replacement {
                replacement: ReplacementOperation {
                    source: ReplacementSource {
                        owner: owner_index(owner)?,
                        index: purchase_index(index)?,
                    },
                    replacement: Some(ReplacementPurchase {
                        pool: (*pool).into(),
                        selection: form_choice(talent, form)?,
                    }),
                },
            })
        }
        ["replace", owner, index, pool, kind, id] => {
            AdvancementInput::Preview(AdvancementProposal::Replacement {
                replacement: ReplacementOperation {
                    source: ReplacementSource {
                        owner: owner_index(owner)?,
                        index: purchase_index(index)?,
                    },
                    replacement: Some(ReplacementPurchase {
                        pool: (*pool).into(),
                        selection: choice(kind, id)?,
                    }),
                },
            })
        }
        ["details", section] => AdvancementInput::Details(match *section {
            "summary" => AdvancementPreviewSection::Summary,
            "removed" => AdvancementPreviewSection::Removed,
            "refunds" => AdvancementPreviewSection::Refunds,
            "balances" => AdvancementPreviewSection::Balances,
            "skills" => AdvancementPreviewSection::Skills,
            "talents" => AdvancementPreviewSection::Talents,
            "expertises" => AdvancementPreviewSection::Expertises,
            "conditions" => AdvancementPreviewSection::RemovedConditions,
            _ => return Err(HELP.into()),
        }),
        _ => return Err(HELP.into()),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    context: ReplyContext,
    request_id: String,
    revision: u64,
    query: AdvancementQuery,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirmation {
    pub basis: AdvancementBasis,
    pub revision: u64,
    pub proposal: AdvancementProposal,
    pub summary: AdvancementSummary,
}
/// Four complete pages at most, including the pinned overview, and one immutable
/// confirmation record. There is no growing catalog or cursor history.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdvancementPages {
    context: Option<ReplyContext>,
    pages: VecDeque<AdvancementPage>,
    current: Option<AdvancementQuery>,
    /// Offset-only trail, bounded by the largest legal section (4096 rows).
    /// It is cleared on every section/proposal/basis change, never a catalog.
    trail: Vec<u16>,
    pending: Option<Pending>,
    confirmation: Option<Confirmation>,
    stale: bool,
    showing_supplies: bool,
}
impl AdvancementPages {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn retained_pages(&self) -> usize {
        self.pages.len()
    }
    /// Whether retained private pages still belong to this exact disclosed state.
    /// This does not imply that a paid intention is currently admissible.
    pub fn is_current(&self, context: &ReplyContext) -> bool {
        !self.stale && self.context.as_ref() == Some(context)
    }
    /// The exact-supply chooser overlays, rather than replaces, a retained page.
    pub fn shows_supplies(&self) -> bool {
        self.showing_supplies
    }
    pub fn confirmation(&self) -> Option<&Confirmation> {
        self.confirmation.as_ref()
    }
    pub fn page(&self) -> Option<&AdvancementPage> {
        self.current
            .as_ref()
            .and_then(|query| self.pages.iter().find(|page| &page.query == query))
    }
    pub fn overview(&self) -> Option<&AdvancementOverview> {
        self.pages
            .iter()
            .find_map(|page| match page.entries.as_slice() {
                [AdvancementEntry::Overview { value }] => Some(value.as_ref()),
                _ => None,
            })
    }
    pub fn observe(&mut self, context: &ReplyContext, has_control: bool) {
        if !has_control
            || self.context.as_ref().is_some_and(|old| {
                old.actor != context.actor
                    || old.branch != context.branch
                    || old.input.stream != context.input.stream
            })
        {
            self.clear();
        } else if self.context.as_ref().is_some_and(|old| old != context) {
            self.stale = true;
            self.pending = None;
        }
    }
    pub fn invalidate(&mut self) {
        self.stale = true;
        self.pending = None;
    }
    fn fresh(&self, context: &ReplyContext) -> Result<(), String> {
        if !self.is_current(context) {
            return Err(STALE.into());
        }
        Ok(())
    }
    fn basis(&self) -> Result<&AdvancementBasis, String> {
        self.pages
            .front()
            .map(|page| &page.basis)
            .ok_or_else(|| "Open advancement first.".into())
    }
    /// Register the exact request before send, including raw ordinary headless
    /// queries. Concurrent requests cannot ambiguously replace private pages.
    pub fn track(
        &mut self,
        context: &ReplyContext,
        id: &str,
        revision: u64,
        query: &AdvancementQuery,
    ) -> Result<(), String> {
        if revision != context.revision {
            return Err(STALE.into());
        }
        if let Some(basis) = query_basis(query) {
            if !basis_matches(basis, context) {
                return Err(STALE.into());
            }
            if self.context.is_some() {
                self.fresh(context)?;
                if self.basis()? != basis {
                    return Err(STALE.into());
                }
            }
        }
        if self.pending.is_some() {
            return Err("Wait for the current advancement page.".into());
        }
        self.pending = Some(Pending {
            context: context.clone(),
            request_id: id.into(),
            revision,
            query: query.clone(),
        });
        Ok(())
    }
    /// Validate everything before mutation. The complete escaped envelope, not
    /// merely its entries, must fit both the feature and negotiated frame limit.
    pub fn validate_reply(
        &self,
        context: &ReplyContext,
        id: &str,
        page: &AdvancementPage,
        frame_limit: usize,
    ) -> Result<(), String> {
        let pending = self
            .pending
            .as_ref()
            .ok_or("Unrequested advancement page.")?;
        if pending.request_id != id
            || &pending.context != context
            || pending.query != page.query
            || pending.revision != page.revision
            || page.revision != context.revision
            || !basis_matches(&page.basis, context)
        {
            return Err("Advancement page does not match its exact request.".into());
        }
        if query_basis(&page.query).is_some_and(|basis| basis != &page.basis) {
            return Err(STALE.into());
        }
        page.validate()
            .map_err(|_| "Malformed advancement page.".to_owned())?;
        if self
            .page()
            .is_some_and(|old| same_section(&old.query, &page.query) && old.total != page.total)
        {
            return Err("Advancement section total changed during traversal.".into());
        }
        if let (
            Some(confirmation),
            AdvancementQuery::Preview {
                proposal, section, ..
            },
        ) = (&self.confirmation, &page.query)
        {
            if &confirmation.proposal == proposal {
                let expected = match section {
                    AdvancementPreviewSection::Removed => Some(confirmation.summary.removed_count),
                    AdvancementPreviewSection::Refunds => Some(confirmation.summary.refund_count),
                    AdvancementPreviewSection::RemovedConditions => {
                        Some(confirmation.summary.removed_condition_count)
                    }
                    _ => None,
                };
                if expected.is_some_and(|total| total != page.total) {
                    return Err(
                        "Advancement detail does not match the confirmed preview counts.".into(),
                    );
                }
            }
        }
        let envelope = ServerMessage::AdvancementPage {
            context: context.clone(),
            request_id: id.into(),
            page: Box::new(page.clone()),
        };
        encode_bounded_json(&envelope, frame_limit.min(MAX_ADVANCEMENT_PAGE_BYTES))
            .map_err(|_| "Advancement response exceeds the complete frame limit.")?;
        Ok(())
    }
    pub fn accept(
        &mut self,
        context: &ReplyContext,
        id: &str,
        page: &AdvancementPage,
        frame_limit: usize,
    ) -> Result<(), String> {
        self.validate_reply(context, id, page, frame_limit)?;
        if matches!(page.query, AdvancementQuery::Overview {}) {
            self.clear();
        }
        if let AdvancementQuery::Preview { proposal, .. } = &page.query {
            if self
                .confirmation
                .as_ref()
                .is_some_and(|old| &old.proposal != proposal)
            {
                self.confirmation = None;
            }
        }
        if let [AdvancementEntry::Summary { value }] = page.entries.as_slice() {
            self.confirmation = Some(Confirmation {
                basis: page.basis.clone(),
                revision: page.revision,
                proposal: value.proposal.clone(),
                summary: value.clone(),
            });
        }
        if !self
            .current
            .as_ref()
            .is_some_and(|query| same_section(query, &page.query))
        {
            self.trail.clear();
        }
        let start = offset(&page.query);
        self.trail.retain(|old| *old < start);
        self.trail.push(start);
        // Strict progress and section bounds make this unreachable for valid
        // responses, but keep the allocation bound explicit at this seam.
        debug_assert!(self.trail.len() <= MAX_ADVANCEMENT_CATALOG_ROWS);
        self.pages.retain(|old| old.query != page.query);
        while self.pages.len() >= 4 {
            let remove = usize::from(matches!(
                self.pages.front().map(|p| &p.query),
                Some(AdvancementQuery::Overview {})
            ));
            self.pages.remove(remove);
        }
        self.pages.push_back(page.clone());
        self.current = Some(page.query.clone());
        self.context = Some(context.clone());
        self.pending = None;
        self.stale = false;
        self.showing_supplies = false;
        Ok(())
    }
    pub fn reject(&mut self, id: &str) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.request_id == id)
        {
            self.invalidate();
        }
    }
    fn query(&self, context: &ReplyContext, query: AdvancementQuery) -> Result<Command, String> {
        self.fresh(context)?;
        Ok(Command::Advancement {
            expected_revision: context.revision,
            query,
        })
    }
    pub fn next(&self, context: &ReplyContext) -> Result<Command, String> {
        let page = self.page().ok_or("Open an advancement page first.")?;
        self.query(
            context,
            with_offset(&page.query, page.next.ok_or("This is the last page.")?)?,
        )
    }
    pub fn previous(&self, context: &ReplyContext) -> Result<Command, String> {
        let page = self.page().ok_or("Open an advancement page first.")?;
        let offset = offset(&page.query);
        if offset == 0 {
            return Err("This is the first page.".into());
        }
        // Exact previously returned offsets handle byte-short pages. Raw
        // nonzero starts may return to zero, rather than invent a page size.
        let previous = self
            .trail
            .iter()
            .copied()
            .filter(|old| *old < offset)
            .max()
            .unwrap_or(0);
        self.query(context, with_offset(&page.query, previous)?)
    }
    pub fn request(
        &mut self,
        input: AdvancementInput,
        state: &crate::ClientState,
    ) -> Result<Option<Request>, String> {
        if !state.has_control() {
            self.clear();
            return Err("Advancement requires control of your character.".into());
        }
        let context = state.snapshot().reply_context();
        self.observe(&context, true);
        let command = match input {
            AdvancementInput::Overview => {
                self.clear();
                Command::Advancement {
                    expected_revision: context.revision,
                    query: AdvancementQuery::Overview {},
                }
            }
            AdvancementInput::Next => self.next(&context)?,
            AdvancementInput::Previous => self.previous(&context)?,
            AdvancementInput::Confirm(text) => {
                let supplies = text.map_or(Ok(SupplySelection::Carried), |text| {
                    self.supplies_from_text(&state.state().observation, &text)
                })?;
                if !state.can_admit_intention() {
                    return Err(
                        "Finish or cancel existing work and wait out recovery first.".into(),
                    );
                }
                self.confirm(&context, &state.state().observation, supplies)?
            }
            AdvancementInput::Supplies => {
                self.fresh(&context)?;
                self.overview().ok_or("Open advancement first.")?;
                self.showing_supplies = true;
                return Ok(None);
            }
            AdvancementInput::Resume | AdvancementInput::Cancel => {
                self.fresh(&context)?;
                let progress = self
                    .overview()
                    .and_then(|value| value.respec.as_ref())
                    .ok_or(
                        "No retained respec work. Open advancement to check current progress.",
                    )?;
                let resume = matches!(input, AdvancementInput::Resume);
                let permitted = if resume {
                    &state.readiness().resume
                } else {
                    &state.readiness().cancel
                };
                if !permitted.contains(&progress.intention) || (resume && progress.active) {
                    return Err("The server has not permitted that respec control.".into());
                }
                if resume {
                    Command::ResumeIntention {
                        expected_revision: context.revision,
                        intention: progress.intention.clone(),
                    }
                } else {
                    Command::CancelIntention {
                        expected_revision: context.revision,
                        intention: progress.intention.clone(),
                        step: None,
                    }
                }
            }
            input => {
                self.fresh(&context)?;
                let basis = self.basis()?.clone();
                let query = match input {
                    AdvancementInput::IntellectExpertises => {
                        AdvancementQuery::IntellectExpertises { basis, offset: 0 }
                    }
                    AdvancementInput::Ledger => AdvancementQuery::Ledger { basis, offset: 0 },
                    AdvancementInput::Balances => AdvancementQuery::Balances { basis, offset: 0 },
                    AdvancementInput::Catalog(catalog) => AdvancementQuery::Catalog {
                        basis,
                        catalog,
                        offset: 0,
                    },
                    AdvancementInput::Preview(proposal) => {
                        // A refused new preview can never leave an older proposal confirmable.
                        self.confirmation = None;
                        AdvancementQuery::Preview {
                            basis,
                            proposal,
                            section: AdvancementPreviewSection::Summary,
                            offset: 0,
                        }
                    }
                    AdvancementInput::Details(section) => AdvancementQuery::Preview {
                        basis,
                        proposal: self
                            .confirmation
                            .as_ref()
                            .ok_or("Preview one exact proposal first.")?
                            .proposal
                            .clone(),
                        section,
                        offset: 0,
                    },
                    _ => unreachable!("handled above"),
                };
                self.query(&context, query)?
            }
        };
        Ok(Some(state.command_request(command)))
    }
    pub fn confirm(
        &self,
        context: &ReplyContext,
        view: &Observation,
        supplies: SupplySelection,
    ) -> Result<Command, String> {
        self.fresh(context)?;
        let confirmation = self
            .confirmation
            .as_ref()
            .ok_or("Preview one exact proposal before confirming.")?;
        let overview = self.overview().ok_or("Open advancement first.")?;
        if !overview.ready || overview.other_preparation || overview.respec.is_some() {
            return Err("Finish or cancel existing work and wait out recovery first; use resume for paused respec.".into());
        }
        let action = match &confirmation.proposal {
            AdvancementProposal::IntellectExpertise { choice } => {
                if supplies != SupplySelection::Carried {
                    return Err("An Intellect expertise choice does not select a ration.".into());
                }
                Action::ChooseIntellectExpertise {
                    basis: Box::new(confirmation.basis.clone()),
                    choice: choice.clone(),
                }
            }
            AdvancementProposal::GrowthSource { source } => {
                if supplies != SupplySelection::Carried {
                    return Err("Choosing future growth does not select a ration.".into());
                }
                Action::ChooseGrowthSource {
                    basis: Box::new(confirmation.basis.clone()),
                    source: source.clone(),
                }
            }
            AdvancementProposal::Purchase { purchase } => {
                if supplies != SupplySelection::Carried {
                    return Err("A purchase does not select a ration.".into());
                }
                Action::Purchase {
                    basis: Box::new(confirmation.basis.clone()),
                    purchase: purchase.clone(),
                }
            }
            AdvancementProposal::Replacement { replacement } => {
                if overview.perceived_threat || overview.received_loss_remaining > 0 {
                    return Err("Respec is not ready: check perceived danger and the received-loss quiet interval.".into());
                }
                self.validate_supplies(view, &supplies)?;
                Action::Respec {
                    basis: Box::new(confirmation.basis.clone()),
                    replacement: replacement.clone(),
                    supplies,
                }
            }
        };
        Ok(Command::Act {
            expected_revision: confirmation.revision,
            action,
        })
    }
    pub fn supply_costs(&self) -> Result<Vec<AbilitySupplyCost>, String> {
        let ration = &self.overview().ok_or("Open advancement first.")?.ration;
        Ok(vec![AbilitySupplyCost {
            supply: ration.supply.id.clone(),
            name: ration.supply.name.clone(),
            quantity: ration.quantity,
            access: ration.access,
        }])
    }
    pub fn supply_candidates(&self, view: &Observation) -> Vec<crate::abilities::SupplyCandidate> {
        self.supply_costs()
            .map(|costs| crate::abilities::supply_candidates_for_costs(view, &costs))
            .unwrap_or_default()
    }
    pub fn supplies_from_text(
        &self,
        view: &Observation,
        text: &str,
    ) -> Result<SupplySelection, String> {
        crate::abilities::selection_from_text_for_costs(view, &self.supply_costs()?, text)
    }
    pub fn validate_supplies(
        &self,
        view: &Observation,
        supplies: &SupplySelection,
    ) -> Result<(), String> {
        if let SupplySelection::Explicit { items } = supplies {
            if items.len() != 1 || items[0].cost_ordinal != 0 {
                return Err("Choose exactly one ration source at cost ordinal 0.".into());
            }
            let checked = crate::abilities::selection_from_items_for_costs(
                view,
                &self.supply_costs()?,
                &[items[0].item],
            )?;
            if &checked != supplies {
                return Err("The exact ration selection changed.".into());
            }
        }
        Ok(())
    }
    pub fn display(&self, view: &Observation) -> Vec<String> {
        if self.showing_supplies {
            let mut rows = vec!["Ration: automatic carried inventory, or one exact visible source. Physical quantities do not promise availability.".into()];
            rows.extend(self.supply_candidates(view).iter().map(|row| {
                format!(
                    "{} #{} x{} ({})",
                    row.name,
                    row.item,
                    row.quantity,
                    if row.carried {
                        "carried"
                    } else {
                        "touch-ground"
                    }
                )
            }));
            rows
        } else {
            self.lines()
        }
    }
    pub fn lines(&self) -> Vec<String> {
        let mut rows = vec!["ADVANCEMENT".into()];
        if self.stale {
            rows.push(STALE.into());
        }
        if let Some(page) = self.page() {
            rows.push(format!(
                "{}: rows {}–{} of {}. {}",
                query_label(&page.query),
                if page.total == 0 {
                    0
                } else {
                    usize::from(offset(&page.query)) + 1
                },
                usize::from(offset(&page.query)) + page.entries.len(),
                page.total,
                if page.next.is_some() {
                    "Next available."
                } else {
                    "End of section."
                }
            ));
            for entry in &page.entries {
                render_entry(entry, &mut rows);
            }
        } else {
            rows.push("Open overview to fetch your character's advancement.".into());
        }
        rows
    }
}
fn basis_matches(basis: &AdvancementBasis, context: &ReplyContext) -> bool {
    basis.actor == context.actor
        && basis.branch == context.branch
        && basis.stream == context.input.stream.stream
        && basis.reset == context.input.stream.epoch
}
fn query_basis(query: &AdvancementQuery) -> Option<&AdvancementBasis> {
    match query {
        AdvancementQuery::Overview {} => None,
        AdvancementQuery::Ledger { basis, .. }
        | AdvancementQuery::IntellectExpertises { basis, .. }
        | AdvancementQuery::Balances { basis, .. }
        | AdvancementQuery::Catalog { basis, .. }
        | AdvancementQuery::Preview { basis, .. } => Some(basis),
    }
}
fn offset(query: &AdvancementQuery) -> u16 {
    match query {
        AdvancementQuery::Overview {} => 0,
        AdvancementQuery::Ledger { offset, .. }
        | AdvancementQuery::IntellectExpertises { offset, .. }
        | AdvancementQuery::Balances { offset, .. }
        | AdvancementQuery::Catalog { offset, .. }
        | AdvancementQuery::Preview { offset, .. } => *offset,
    }
}
fn with_offset(query: &AdvancementQuery, value: u16) -> Result<AdvancementQuery, String> {
    let mut result = query.clone();
    match &mut result {
        AdvancementQuery::Overview {}
        | AdvancementQuery::Preview {
            section: AdvancementPreviewSection::Summary,
            ..
        } => return Err("This section has one page.".into()),
        AdvancementQuery::Ledger { offset, .. }
        | AdvancementQuery::IntellectExpertises { offset, .. }
        | AdvancementQuery::Balances { offset, .. }
        | AdvancementQuery::Catalog { offset, .. }
        | AdvancementQuery::Preview { offset, .. } => *offset = value,
    }
    Ok(result)
}
fn same_section(left: &AdvancementQuery, right: &AdvancementQuery) -> bool {
    matches!((with_offset(left, 0), with_offset(right, 0)), (Ok(a), Ok(b)) if a == b)
}
fn query_label(query: &AdvancementQuery) -> String {
    match query {
        AdvancementQuery::Overview {} => "Overview".into(),
        AdvancementQuery::IntellectExpertises { .. } => {
            "Intellect expertise thresholds (browse expertises for candidate IDs)".into()
        }
        AdvancementQuery::Ledger { .. } => "Purchase ledger".into(),
        AdvancementQuery::Balances { .. } => "Point balances".into(),
        AdvancementQuery::Catalog { catalog, .. } => {
            format!("{catalog:?} catalog (eligibility requires preview)")
        }
        AdvancementQuery::Preview { section, .. } => format!("Preview {section:?}"),
    }
}
fn choice_label(choice: &PurchaseChoice) -> String {
    match choice {
        PurchaseChoice::SkillRank { skill } => format!("skill {skill}"),
        PurchaseChoice::Talent { talent } => format!("talent {talent}"),
        PurchaseChoice::Expertise { expertise } => format!("expertise {expertise}"),
        PurchaseChoice::TalentForm { talent, form } => format!("talent {talent} form {form}"),
    }
}
fn growth_source_label(source: &GrowthSource) -> String {
    match source {
        GrowthSource::Racial {} => "racial".into(),
        GrowthSource::Class { class } => format!("class {class}"),
    }
}
fn future_growth_label(future: &FutureGrowth) -> String {
    match future {
        FutureGrowth::Disabled {} => "disabled; earned XP retained".into(),
        FutureGrowth::Authored {} => "authored future source".into(),
        FutureGrowth::Selected { source } => growth_source_label(source),
    }
}
fn hd_ordinal_label(owner: u64) -> String {
    owner
        .checked_add(1)
        .map_or_else(|| "invalid".into(), |ordinal| ordinal.to_string())
}

pub fn proposal_label(proposal: &AdvancementProposal) -> String {
    match proposal {
        AdvancementProposal::IntellectExpertise { choice } => format!(
            "Intellect threshold {}: expertise {}",
            choice.threshold, choice.expertise
        ),
        AdvancementProposal::GrowthSource { source } => {
            format!("Future growth: {}", growth_source_label(source))
        }
        AdvancementProposal::Purchase { purchase } => format!(
            "Purchase HD {} / pool {} / {}",
            hd_ordinal_label(purchase.owner),
            purchase.pool,
            choice_label(&purchase.selection)
        ),
        AdvancementProposal::Replacement { replacement } => format!(
            "Replace HD {} / purchase {} with {}",
            hd_ordinal_label(replacement.source.owner),
            u32::from(replacement.source.index) + 1,
            replacement.replacement.as_ref().map_or_else(
                || "nothing (refund only)".into(),
                |value| format!("pool {} / {}", value.pool, choice_label(&value.selection))
            )
        ),
    }
}
fn render_entry(entry: &AdvancementEntry, rows: &mut Vec<String>) {
    match entry {
        AdvancementEntry::Overview { value } => {
            if value.strength_acquired > 0 { rows.push(format!("Permanent potion Strength owned: {} (not refundable; capped effect)", value.strength_acquired)); }
            rows.push(format!("Starting skills: {} ({}), {} ({})", value.starting_skills[0].name, value.starting_skills[0].id, value.starting_skills[1].name, value.starting_skills[1].id));
            rows.push(format!("Points: earned {}, spent {}, unspent {}", value.points.earned, value.points.spent, value.points.unspent));
            rows.push(format!("XP: held {} / next threshold {}; total earned {}, spent {}; highest earned HD {}; peak threat {}", value.growth.held_xp, value.growth.next_threshold, value.growth.earned_xp, value.growth.spent_xp, value.growth.highest_earned_tier, value.growth.peak_threat));
            rows.push(format!("Restorable Hit Dice {}; highest applied HD {}", value.growth.restorable_hit_dice, value.growth.highest_applied_tier));
            match &value.growth.restoration_blocker {
                Some(HitDieRestorationBlocker::MissingSource {}) => rows.push("Select a growth/restoration source before restoring a lost Hit Die.".into()),
                Some(HitDieRestorationBlocker::MissingHealthRoot {}) => rows.push("Hit Die restoration is unavailable because this recorded build has no original Health root.".into()),
                None => {}
            }
            rows.push(format!("Future growth: {}. Provisional XP policy. Use source racial or source class <class ID> to preview a change.", future_growth_label(&value.growth.future)));
            rows.push(format!("Ready: {}; perceived danger: {}; received-loss quiet remaining: {} active ticks; other preparation: {}", value.ready, value.perceived_threat, value.received_loss_remaining, value.other_preparation));
            rows.push(format!("{}Timing: purchase recovery {}; respec preparation {}, recovery {}; received-loss quiet interval {} active ticks", if value.timings.provisional { "Provisional " } else { "" }, value.timings.purchase_recovery, value.timings.respec_prepare, value.timings.respec_recovery, value.timings.received_loss_interval));
            rows.push(format!("Respec needs {} {} ({}): {:?}. Availability is checked by the server.", value.ration.quantity, value.ration.supply.name, value.ration.supply.id, value.ration.access));
            if let Some(progress) = &value.respec { rows.push(format!("Respec {}: {} of {} active ticks remaining. {}. Use resume for paused work or cancel to release an unspent ration.", if progress.active { "active" } else { "paused" }, progress.remaining_ticks, progress.total_ticks, proposal_label(&AdvancementProposal::Replacement { replacement: progress.replacement.clone() }))); }
            rows.push("Browse ledger, balances, intellect-expertises and catalogs; preview an exact purchase, replacement or future growth source before confirming.".into());
        }
        AdvancementEntry::Purchase { value } | AdvancementEntry::Removed { value } => rows.push(format!("HD {} / purchase {} / {} ({}) / {} [{}] / {:?}", value.source.owner.checked_add(1).expect("validated HD ordinal"), u32::from(value.source.index) + 1, value.pool.name, value.pool.id, value.name, choice_label(&value.selection), value.status)),
        AdvancementEntry::Balance { value } => rows.push(format!("HD {} / {} ({}) / granted {}, spent {}, unspent {} / {:?}", value.owner.checked_add(1).expect("validated HD ordinal"), value.pool.name, value.pool.id, value.granted, value.spent, value.unspent, value.allowed)),
        AdvancementEntry::Catalog { value } => {
            rows.push(format!("{} [{}]", value.name, choice_label(&value.selection)));
            for form in &value.form_choices {
                rows.push(format!("  {} [form {}]", form.name, form.id));
            }
        },
        AdvancementEntry::Summary { value } => {
            if value.strength_acquired > 0 { rows.push(format!("Permanent potion Strength owned: {} (not refundable; capped effect)", value.strength_acquired)); }
            rows.push(proposal_label(&value.proposal));
            rows.push(format!("Resulting build {}; removed purchases {}; gross refund rows {}; removed conditions {}", value.resulting_build_epoch, value.removed_count, value.refund_count, value.removed_condition_count));
            rows.push(format!("{}Health: {}/{}; injury {}", if value.health.dead { "DEATH. " } else { "" }, value.health.current, value.health.maximum, value.health.injury));
            rows.push(format!("Health recovery: period {}, elapsed {}, stopped {}", value.health_recovery.period_ticks, value.health_recovery.elapsed_ticks, value.health_recovery.stopped));
            for resource in [&value.resources.stamina, &value.resources.focus, &value.resources.mana] { rows.push(format!("{} ({}): {}/{}", resource.resource.name, resource.resource.id, resource.balance, resource.maximum)); }
            rows.push(format!("Canceled resource holds: {}. Values include resulting clamping and death consequences.", value.canceled_resource_holds));
            rows.push("Review details removed/refunds/balances/skills/talents/expertises/conditions; next and previous traverse every row. Confirm commits this exact proposal.".into());
        }
        AdvancementEntry::Refund { value } => rows.push(format!("HD {} / {} ({}): {} gross points returned; resulting balances show replacement spending", value.owner.checked_add(1).expect("validated HD ordinal"), value.pool.name, value.pool.id, value.points)),
        AdvancementEntry::Skill { value } => rows.push(format!("{} ({}): permanent rank {}", value.skill.name, value.skill.id, value.rank)),
        AdvancementEntry::IntellectExpertise { value } => {
            let selection = value.expertise.as_ref().map_or_else(
                || if value.status == AdvancementStatus::Active { "available; browse expertises, then intellect-expertise <threshold> <ID>".into() } else { "unavailable".into() },
                |expertise| format!("{} ({}); retained choice, cannot reselect", expertise.name, expertise.id),
            );
            rows.push(format!("Intellect threshold {}: {:?}; {}", value.threshold, value.status, selection));
        },
        AdvancementEntry::Expertise { value } => rows.extend(crate::stats::expertise_lines(value)),
        AdvancementEntry::Talent { value } => rows.push(format!("{} ({}): {:?}", value.talent.name, value.talent.id, value.status)),
        AdvancementEntry::RemovedCondition { value } => rows.push(format!("Lost condition {} ({}): {} applications, {} stacks", value.condition.name, value.condition.id, value.applications, value.stacks)),
    }
}
#[cfg(test)]
#[path = "advancement/tests.rs"]
mod tests;
