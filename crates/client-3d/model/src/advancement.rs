//! Bounded, typed advancement presentation. The shared reducer owns admission.
use tor_client_common::{advancement::AdvancementPages, ClientState};
use tor_protocol::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdvancementView {
    pub context: Option<ReplyContext>,
    pub page: Option<AdvancementPage>,
    pub confirmation: Option<tor_client_common::advancement::Confirmation>,
    /// The common supply chooser temporarily overlays a retained protocol page.
    pub supplies: bool,
    pub lines: Vec<String>,
}
impl AdvancementView {
    pub fn new(pages: &AdvancementPages, state: &ClientState, synchronized: bool) -> Self {
        if !state.has_control() {
            return Self {
                lines: vec!["Advancement requires control of your character.".into()],
                ..Self::default()
            };
        }
        let context = state.reply_context();
        let current = synchronized && pages.is_current(&context);
        Self {
            supplies: current && pages.shows_supplies(),
            context: current.then_some(context),
            page: current.then(|| pages.page().cloned()).flatten(),
            confirmation: current.then(|| pages.confirmation().cloned()).flatten(),
            lines: pages.display(&state.state().observation),
        }
    }
    pub fn current_page(&self, shown: &ReplyContext) -> Option<&AdvancementPage> {
        self.page.as_ref().filter(|page| {
            self.context.as_ref() == Some(shown)
                && page.revision == shown.revision
                && page.basis.actor == shown.actor
                && page.basis.branch == shown.branch
                && page.basis.stream == shown.input.stream.stream
                && page.basis.reset == shown.input.stream.epoch
        })
    }
    /// Confirmation remains explicit and is rechecked by the shared worker.
    pub fn confirm_command(&self, shown: &ReplyContext, can_act: bool) -> Option<&'static str> {
        let page = self.current_page(shown)?;
        let confirmation = self.confirmation.as_ref()?;
        (can_act
            && confirmation.revision == shown.revision
            && confirmation.basis == page.basis
            && confirmation.summary.proposal == confirmation.proposal)
            .then_some("confirm")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdvancementAction {
    Pool {
        row: AdvancementBalanceRow,
        kind: AdvancementKind,
    },
    Preview(PurchaseChoice),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdvancementOption {
    pub label: String,
    pub action: AdvancementAction,
    pub available: bool,
}
#[derive(Clone, Debug)]
struct Funding {
    context: ReplyContext,
    basis: AdvancementBasis,
    row: AdvancementBalanceRow,
}
#[derive(Clone, Debug, Default)]
pub struct AdvancementBrowser {
    funding: Option<Funding>,
}
pub fn catalog_command(kind: AdvancementKind) -> &'static str {
    match kind {
        AdvancementKind::SkillRank => "skills",
        AdvancementKind::Talent => "talents",
        AdvancementKind::Expertise => "expertises",
    }
}
fn selection_text(choice: &PurchaseChoice) -> String {
    match choice {
        PurchaseChoice::SkillRank { skill } => format!("skill {skill}"),
        PurchaseChoice::Talent { talent } => format!("talent {talent}"),
        PurchaseChoice::TalentForm { talent, form } => format!("talent {talent} form {form}"),
        PurchaseChoice::Expertise { expertise } => format!("expertise {expertise}"),
    }
}
fn choices(row: &AdvancementCatalogRow) -> Vec<(String, PurchaseChoice)> {
    if let PurchaseChoice::Talent { talent } = &row.selection {
        if !row.form_choices.is_empty() {
            return row
                .form_choices
                .iter()
                .map(|form| {
                    (
                        format!("{} — {}", row.name, form.name),
                        PurchaseChoice::TalentForm {
                            talent: talent.clone(),
                            form: form.id.clone(),
                        },
                    )
                })
                .collect();
        }
    }
    vec![(row.name.clone(), row.selection.clone())]
}
impl AdvancementBrowser {
    pub fn reconcile(&mut self, view: &AdvancementView, shown: &ReplyContext) {
        if self.funding(view, shown).is_none() {
            self.funding = None;
        }
    }
    pub fn funding(
        &self,
        view: &AdvancementView,
        shown: &ReplyContext,
    ) -> Option<&AdvancementBalanceRow> {
        let page = view.current_page(shown)?;
        self.funding
            .as_ref()
            .filter(|funding| funding.context == *shown && funding.basis == page.basis)
            .map(|funding| &funding.row)
    }
    pub fn options(&self, view: &AdvancementView, shown: &ReplyContext) -> Vec<AdvancementOption> {
        let Some(page) = view.current_page(shown) else {
            return Vec::new();
        };
        if view.supplies {
            return Vec::new();
        }
        let mut result = Vec::new();
        for entry in &page.entries {
            match (&page.query, entry) {
                (AdvancementQuery::Balances { .. }, AdvancementEntry::Balance { value }) => {
                    let Some(owner) = value.owner.checked_add(1) else {
                        continue;
                    };
                    for kind in &value.allowed {
                        result.push(AdvancementOption {
                            label: format!(
                                "HD {owner} · {} — {} unspent / {} granted · {}",
                                value.pool.name,
                                value.unspent,
                                value.granted,
                                catalog_command(*kind)
                            ),
                            action: AdvancementAction::Pool {
                                row: value.clone(),
                                kind: *kind,
                            },
                            available: true,
                        });
                    }
                }
                (AdvancementQuery::Catalog { .. }, AdvancementEntry::Catalog { value }) => {
                    for (label, selection) in choices(value) {
                        let available = self
                            .funding(view, shown)
                            .is_some_and(|row| row.allowed.contains(&selection.kind()));
                        result.push(AdvancementOption {
                            label,
                            action: AdvancementAction::Preview(selection),
                            available,
                        });
                    }
                }
                _ => {}
            }
        }
        result
    }
    /// An option requests a catalog or a preview. It can never commit a purchase.
    pub fn choose(
        &mut self,
        view: &AdvancementView,
        shown: &ReplyContext,
        action: &AdvancementAction,
    ) -> Option<String> {
        let page = view.current_page(shown)?;
        if view.supplies {
            return None;
        }
        match action {
            AdvancementAction::Pool { row, kind } => {
                if !matches!(page.query, AdvancementQuery::Balances { .. }) || row.owner.checked_add(1).is_none() || !row.allowed.contains(kind)
                    || !page.entries.iter().any(|entry| matches!(entry, AdvancementEntry::Balance { value } if value == row)) { return None; }
                self.funding = Some(Funding {
                    context: shown.clone(),
                    basis: page.basis.clone(),
                    row: row.clone(),
                });
                Some(catalog_command(*kind).into())
            }
            AdvancementAction::Preview(selection) => {
                let funding = self.funding(view, shown)?;
                if !matches!(page.query, AdvancementQuery::Catalog { .. }) || !funding.allowed.contains(&selection.kind())
                    || !page.entries.iter().any(|entry| matches!(entry, AdvancementEntry::Catalog { value } if choices(value).iter().any(|(_, choice)| choice == selection))) { return None; }
                Some(format!(
                    "purchase {} {} {}",
                    funding.owner.checked_add(1)?,
                    funding.pool.id,
                    selection_text(selection)
                ))
            }
        }
    }
}
