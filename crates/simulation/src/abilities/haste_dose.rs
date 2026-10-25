//! Closed duration receipt for one Haste dose; zero overdose rest is not a timer.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HasteDoseOutcome {
    Hasted { duration: u64 },
    Overdose { rest: u64 },
}
impl HasteDoseOutcome {
    pub fn valid(self) -> bool {
        match self {
            Self::Hasted { duration } => (1..=crate::limits::NUMERIC as u64).contains(&duration),
            Self::Overdose { rest } => rest <= crate::limits::NUMERIC as u64,
        }
    }
}
impl<'de> Deserialize<'de> for HasteDoseOutcome {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Hasted { duration: u64 },
            Overdose { rest: u64 },
        }
        let outcome = match crate::bounded::named_map(deserializer)? {
            Fields::Hasted { duration } => Self::Hasted { duration },
            Fields::Overdose { rest } => Self::Overdose { rest },
        };
        if !outcome.valid() {
            return Err(serde::de::Error::custom("invalid Haste dose duration"));
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn haste_dose_receipt_is_closed_and_zero_is_only_overdose_rest() {
        for outcome in [
            HasteDoseOutcome::Hasted { duration: 400 },
            HasteDoseOutcome::Overdose { rest: 0 },
            HasteDoseOutcome::Overdose { rest: 700 },
        ] {
            let encoded = serde_json::to_string(&outcome).unwrap();
            assert_eq!(
                serde_json::from_str::<HasteDoseOutcome>(&encoded).unwrap(),
                outcome
            );
        }
        for invalid in [
            r#"{"kind":"hasted","duration":0}"#,
            r#"{"kind":"hasted"}"#,
            r#"{"kind":"overdose"}"#,
            r#"{"kind":"overdose","rest":null}"#,
            r#"{"kind":"overdose","rest":0,"duration":1}"#,
            r#"{"kind":"overdose","rest":0,"rest":1}"#,
            r#"{"kind":"hasted","duration":1000001}"#,
            r#"{"kind":"overdose","rest":1000001}"#,
            r#"{"overdose":{"rest":0}}"#,
            r#"["overdose",0]"#,
            r#""overdose""#,
            "null",
        ] {
            assert!(
                serde_json::from_str::<HasteDoseOutcome>(invalid).is_err(),
                "{invalid}"
            );
        }
    }
}
