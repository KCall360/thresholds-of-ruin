//! Canonical bounded page admission without a protocol dependency.
//! The host must independently test actual serde envelopes against these bounds.
use super::*;

pub const MAX_CHART_PAGE_RECORDS: usize = 512;
pub const MAX_CHART_PAGE_BYTES: usize = 128 * 1024;
pub const CHART_ENVELOPE_BYTES: usize = 8192;
pub const CHART_COMPONENT_BYTES: usize = 96;
pub const CHART_ROW_BYTES: usize = 256;
pub const CHART_MONSTER_MARK_BYTES: usize = 96;
pub const CHART_PRESENCE_MARK_BYTES: usize = 32;
pub const CHART_MAPPING_MARK_BYTES: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageLimits {
    pub records: usize,
    pub bytes: usize,
    pub envelope_bytes: usize,
}
impl PageLimits {
    /// Canonical gameplay admission, independent of any attached client's limits.
    pub const fn canonical() -> Self {
        Self {
            records: MAX_CHART_PAGE_RECORDS,
            bytes: MAX_CHART_PAGE_BYTES,
            envelope_bytes: CHART_ENVELOPE_BYTES,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryError {
    Limits,
    Range,
    Arithmetic,
    RecordTooLarge,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSpan {
    pub end: usize,
    pub encoded_bytes_bound: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeliverySummary {
    pub records: usize,
    pub pages: usize,
    pub encoded_bytes_bound: usize,
}

/// Covers a flattened tagged wire Row, independent of serde and host metadata.
/// Shared by complete-candidate and page admission.
pub fn row_encoded_size_bound(row: &Row) -> Result<usize, Invalid> {
    let mark = match &row.mark {
        Mark::Monster(a) => {
            if !a.valid() {
                return Err(Invalid::Appearance);
            }
            add(
                CHART_MONSTER_MARK_BYTES,
                add(
                    escaped(&a.name)?,
                    a.asset.as_deref().map_or(Ok(4), escaped)?,
                )?,
            )?
        }
        Mark::Food | Mark::Magic => CHART_PRESENCE_MARK_BYTES,
        Mark::Mapping(_) => CHART_MAPPING_MARK_BYTES,
    };
    add(CHART_ROW_BYTES, mark)
}

impl Chart {
    pub fn record_count(&self) -> Result<usize, DeliveryError> {
        self.components
            .len()
            .checked_add(self.rows.len())
            .ok_or(DeliveryError::Arithmetic)
    }
    pub fn record_encoded_size_bound(&self, ordinal: usize) -> Result<usize, DeliveryError> {
        if ordinal < self.components.len() {
            return Ok(CHART_COMPONENT_BYTES);
        }
        let row = self
            .rows
            .get(ordinal - self.components.len())
            .ok_or(DeliveryError::Range)?;
        row_encoded_size_bound(row).map_err(|_| DeliveryError::Arithmetic)
    }
    /// Greedy canonical prefix. No allocations, world reads, RNG or side effects.
    pub fn page_span(&self, start: usize, limits: PageLimits) -> Result<PageSpan, DeliveryError> {
        if limits.records == 0 || limits.envelope_bytes >= limits.bytes {
            return Err(DeliveryError::Limits);
        }
        let total = self.record_count()?;
        if start >= total {
            return Err(DeliveryError::Range);
        }
        let mut end = start;
        let mut bytes = limits.envelope_bytes;
        while end < total && end - start < limits.records {
            let charged = self
                .record_encoded_size_bound(end)?
                .checked_add(1)
                .ok_or(DeliveryError::Arithmetic)?; // comma for EVERY record
            let Some(next_bytes) = bytes.checked_add(charged) else {
                return Err(DeliveryError::Arithmetic);
            };
            if next_bytes > limits.bytes {
                if end == start {
                    return Err(DeliveryError::RecordTooLarge);
                }
                break;
            }
            end = end.checked_add(1).ok_or(DeliveryError::Arithmetic)?;
            bytes = next_bytes;
        }
        Ok(PageSpan {
            end,
            encoded_bytes_bound: bytes,
        })
    }
    pub fn page_end(&self, start: usize, limits: PageLimits) -> Result<usize, DeliveryError> {
        self.page_span(start, limits).map(|page| page.end)
    }
    /// Full immutable candidate admission witness, before any identification.
    /// This is not a receipt: policy still owns mutation/outcome/identification.
    /// No total-page cap is imposed; callers charge this checked result against
    /// their separately justified archive/source/work budgets before publishing.
    pub fn delivery_summary(&self, limits: PageLimits) -> Result<DeliverySummary, DeliveryError> {
        let records = self.record_count()?;
        let mut summary = DeliverySummary {
            records,
            pages: 0,
            encoded_bytes_bound: 0,
        };
        let mut start = 0;
        while start < records {
            let page = self.page_span(start, limits)?;
            summary.pages = summary
                .pages
                .checked_add(1)
                .ok_or(DeliveryError::Arithmetic)?;
            summary.encoded_bytes_bound = summary
                .encoded_bytes_bound
                .checked_add(page.encoded_bytes_bound)
                .ok_or(DeliveryError::Arithmetic)?;
            start = page.end;
        }
        Ok(summary)
    }
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;
