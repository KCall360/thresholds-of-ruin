//! Stateless bounded owner pages. Authorization and basis validation are host-owned.
use crate::Failure;
use serde::Serialize;
use tor_protocol as p;

pub(crate) struct PageBudget<'a> {
    pub context: &'a p::ReplyContext,
    pub request_id: &'a str,
    pub limit: usize,
}
impl PageBudget<'_> {
    fn fits(&self, page: &p::AdvancementPage) -> Result<bool, Failure> {
        // Match the complete wire envelope while borrowing the page. Counting
        // must include escaped labels, echoed operands, context and request ID.
        #[derive(Serialize)]
        struct Envelope<'a> {
            #[serde(rename = "type")]
            kind: &'static str,
            context: &'a p::ReplyContext,
            request_id: &'a str,
            page: &'a p::AdvancementPage,
        }
        p::encoded_length(
            &Envelope {
                kind: "advancement_page",
                context: self.context,
                request_id: self.request_id,
                page,
            },
            self.limit.min(p::MAX_ADVANCEMENT_PAGE_BYTES),
        )
        .map(|size| size.is_some())
        .map_err(|_| invalid())
    }
    pub(crate) fn check(&self, page: &p::AdvancementPage) -> Result<(), Failure> {
        if self.fits(page)? {
            Ok(())
        } else {
            Err(limit())
        }
    }
}
fn limit() -> Failure {
    Failure::new(
        p::ErrorCode::ResourceLimit,
        "Advancement detail cannot fit the response limit",
    )
}
fn invalid() -> Failure {
    Failure::new(p::ErrorCode::InvalidArchive, "Invalid advancement detail")
}
/// The caller bounds source traversal and builds only the requested rows.
/// `rows` must begin at the exact requested offset, and contain the whole suffix.
pub(crate) fn pack(
    basis: p::AdvancementBasis,
    revision: u64,
    query: p::AdvancementQuery,
    total: usize,
    rows: impl IntoIterator<Item = Result<p::AdvancementEntry, Failure>>,
    budget: &PageBudget<'_>,
) -> Result<p::AdvancementPage, Failure> {
    let offset = usize::from(query.offset());
    if total > query.row_limit() || offset > total || (total != 0 && offset == total) {
        return Err(Failure::new(
            p::ErrorCode::InvalidRequest,
            "Invalid advancement page offset",
        ));
    }
    let total = u16::try_from(total).map_err(|_| invalid())?;
    let mut page = p::AdvancementPage {
        basis,
        revision,
        query,
        total,
        next: None,
        entries: Vec::new(),
    };
    let mut end = offset;
    let mut truncated_by_budget = false;
    for row in rows.into_iter().take(p::MAX_ADVANCEMENT_PAGE_ENTRIES) {
        if end >= usize::from(total) {
            return Err(invalid());
        }
        page.entries.push(row?);
        end += 1;
        page.next = (end < usize::from(total)).then(|| u16::try_from(end).expect("bounded total"));
        if !budget.fits(&page)? {
            page.entries.pop();
            end -= 1;
            if page.entries.is_empty() {
                return Err(limit());
            }
            page.next = Some(u16::try_from(end).map_err(|_| invalid())?);
            truncated_by_budget = true;
            break;
        }
    }
    if end < usize::from(total)
        && !truncated_by_budget
        && page.entries.len() < p::MAX_ADVANCEMENT_PAGE_ENTRIES
    {
        return Err(invalid());
    }
    if end < usize::from(total) {
        page.next = Some(u16::try_from(end).map_err(|_| invalid())?);
    }
    page.validate().map_err(|_| invalid())?;
    budget.check(&page)?;
    Ok(page)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> p::ReplyContext {
        p::ReplyContext {
            input: p::InputContext {
                stream: p::StreamContext {
                    stream: p::StreamId("\\\"".repeat(64)),
                    epoch: u64::MAX,
                },
                readiness_revision: u64::MAX,
            },
            actor: p::ActorId(u64::MAX - 1),
            branch: p::BranchId("\\\"".repeat(64)),
            cursor: p::StreamCursor {
                sequence: u64::MAX,
                tick: u64::MAX,
            },
            revision: u64::MAX,
        }
    }
    fn basis(context: &p::ReplyContext) -> p::AdvancementBasis {
        p::AdvancementBasis {
            actor: context.actor,
            branch: context.branch.clone(),
            stream: context.input.stream.stream.clone(),
            reset: context.input.stream.epoch,
            build_epoch: u64::MAX,
        }
    }
    fn row(index: u16) -> p::AdvancementEntry {
        p::AdvancementEntry::Purchase {
            value: p::AdvancementLedgerRow {
                source: p::ReplacementSource { owner: 0, index },
                pool: p::AdvancementContentRef {
                    id: "p".repeat(60),
                    name: "\\\"".repeat(128),
                },
                selection: p::PurchaseChoice::Talent {
                    talent: format!("t{:059}", index),
                },
                name: "\\\"".repeat(128),
                status: p::AdvancementStatus::Active,
            },
        }
    }
    fn bytes(context: &p::ReplyContext, request_id: &str, page: &p::AdvancementPage) -> usize {
        serde_json::to_vec(&p::ServerMessage::AdvancementPage {
            context: context.clone(),
            request_id: request_id.into(),
            page: Box::new(page.clone()),
        })
        .unwrap()
        .len()
    }
    #[test]
    fn owner_paging_full_escaped_envelope_adapts_before_publication_and_traverses_every_row() {
        let context = context();
        let basis = basis(&context);
        let request_id = "\\\"".repeat(64);
        let budget = PageBudget {
            context: &context,
            request_id: &request_id,
            limit: 1024 * 1024,
        };
        let mut offset = 0;
        let mut seen = Vec::new();
        let mut maximum = 0;
        loop {
            let query = p::AdvancementQuery::Ledger {
                basis: basis.clone(),
                offset,
            };
            let page = pack(
                basis.clone(),
                context.revision,
                query,
                64,
                (offset..64).map(|i| Ok(row(i))),
                &budget,
            )
            .unwrap();
            let length = bytes(&context, &request_id, &page);
            maximum = maximum.max(length);
            assert!(length <= p::MAX_ADVANCEMENT_PAGE_BYTES);
            assert!(
                page.entries.len() < p::MAX_ADVANCEMENT_PAGE_ENTRIES,
                "escaped full envelope, not just the entry-count cap, must determine packing"
            );
            assert!(p::encode_response(
                &p::ServerMessage::AdvancementPage {
                    context: context.clone(),
                    request_id: request_id.clone(),
                    page: Box::new(page.clone())
                },
                None,
                budget.limit
            )
            .is_ok());
            for entry in &page.entries {
                let p::AdvancementEntry::Purchase { value } = entry else {
                    unreachable!()
                };
                seen.push(value.source.index);
            }
            if let Some(next) = page.next {
                assert!(next > offset);
                offset = next;
            } else {
                break;
            }
        }
        assert_eq!(seen, (0..64).collect::<Vec<_>>());
        eprintln!("escaped full-envelope maximum {maximum} bytes");
    }
    #[test]
    fn owner_paging_actual_singleton_uses_negotiated_budget_with_no_partial_page() {
        let context = context();
        let basis = basis(&context);
        let request_id = "\\\"".repeat(64);
        let query = p::AdvancementQuery::Ledger {
            basis: basis.clone(),
            offset: 0,
        };
        let budget = PageBudget {
            context: &context,
            request_id: &request_id,
            limit: p::MAX_ADVANCEMENT_PAGE_BYTES,
        };
        let page = pack(
            basis.clone(),
            context.revision,
            query.clone(),
            1,
            [Ok(row(0))],
            &budget,
        )
        .unwrap();
        let exact = bytes(&context, &request_id, &page);
        assert!(
            exact < p::MAX_ADVANCEMENT_PAGE_BYTES,
            "legal singleton fits the full global cap"
        );
        let tight = PageBudget {
            limit: exact,
            ..budget
        };
        assert_eq!(
            pack(
                basis.clone(),
                context.revision,
                query.clone(),
                1,
                [Ok(row(0))],
                &tight
            )
            .unwrap(),
            page
        );
        let short = PageBudget {
            limit: exact - 1,
            ..tight
        };
        assert_eq!(
            pack(basis, context.revision, query, 1, [Ok(row(0))], &short)
                .unwrap_err()
                .code,
            p::ErrorCode::ResourceLimit
        );
        eprintln!("legal escaped singleton {exact} bytes; exact-minus-one refused");
    }
    #[test]
    fn owner_paging_nonempty_end_offset_is_client_input_error() {
        let context = context();
        let basis = basis(&context);
        let query = p::AdvancementQuery::Ledger {
            basis: basis.clone(),
            offset: 1,
        };
        let budget = PageBudget {
            context: &context,
            request_id: "end-offset",
            limit: p::MAX_ADVANCEMENT_PAGE_BYTES,
        };
        let error = pack(
            basis,
            context.revision,
            query,
            1,
            std::iter::empty(),
            &budget,
        )
        .unwrap_err();
        assert_eq!(
            error.code,
            p::ErrorCode::InvalidRequest,
            "a supplied offset at a nonempty end is not a corrupt source archive"
        );
    }
}
