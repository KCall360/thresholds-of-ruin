//! Positive item quantities, separate from bounded combat dice. Context is
//! supplied explicitly and compiled before a paid action can consume randomness.
use crate::{abilities::RuleError, GameError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum QuantityExpr {
    Fixed { quantity: u64 },
    Uniform { minimum: u64, maximum: u64 },
    DepthScaledUniform { minimum: u64, maximum_base: u64, per_depth: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuantityRange {
    pub minimum: u64,
    pub maximum: u64,
}

fn invalid() -> GameError { GameError::Rule(RuleError::InvalidDefinition) }

impl QuantityExpr {
    /// Validate every authored operand, including the largest supported depth.
    pub fn valid(self) -> bool {
        self.resolve(Some(1)).is_ok() && self.resolve(Some(u32::MAX)).is_ok()
    }

    pub fn resolve(self, depth: Option<u32>) -> Result<QuantityRange, GameError> {
        let (minimum, maximum) = match self {
            Self::Fixed { quantity } => (quantity, quantity),
            Self::Uniform { minimum, maximum } => (minimum, maximum),
            Self::DepthScaledUniform { minimum, maximum_base, per_depth } => {
                let depth = depth.filter(|depth| *depth > 0).ok_or_else(invalid)?;
                let maximum = u64::from(depth).checked_mul(per_depth)
                    .and_then(|value| maximum_base.checked_add(value)).ok_or_else(invalid)?;
                (minimum, maximum)
            }
        };
        QuantityRange::new(minimum, maximum)
    }
}

impl QuantityRange {
    pub fn new(minimum: u64, maximum: u64) -> Result<Self, GameError> {
        if minimum == 0 || maximum < minimum { return Err(invalid()); }
        Ok(Self { minimum, maximum })
    }

    pub fn valid(self) -> bool { Self::new(self.minimum, self.maximum).is_ok() }

    /// Uniform sampling with an explicit work bound. A fixed amount consumes
    /// no random state. Exhaustion rejects the enclosing unpublished candidate.
    pub fn sample(self, state: &mut u64) -> Result<u64, GameError> {
        if !self.valid() { return Err(invalid()); }
        if self.minimum == self.maximum { return Ok(self.minimum); }
        let width = self.maximum - self.minimum + 1;
        let threshold = width.wrapping_neg() % width;
        for _ in 0..64 {
            let word = crate::dice::random_word(state);
            if word >= threshold { return Ok(self.minimum + word % width); }
        }
        Err(invalid())
    }
}

impl<'de> Deserialize<'de> for QuantityRange {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields { minimum: u64, maximum: u64 }
        let fields: Fields = crate::bounded::named_map(d)?;
        Self::new(fields.minimum, fields.maximum)
            .map_err(|_| serde::de::Error::custom("invalid quantity range"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contextual_quantities_require_depth_and_check_all_arithmetic() {
        let gold = QuantityExpr::DepthScaledUniform { minimum: 2, maximum_base: 51, per_depth: 10 };
        assert!(gold.resolve(None).is_err());
        assert!(gold.resolve(Some(0)).is_err());
        assert_eq!(gold.resolve(Some(1)).unwrap(), QuantityRange { minimum: 2, maximum: 61 });
        assert_eq!(gold.resolve(Some(u32::MAX)).unwrap().maximum, 42_949_673_001);
        assert!(QuantityExpr::DepthScaledUniform { minimum: 2, maximum_base: u64::MAX, per_depth: 1 }.resolve(Some(1)).is_err());
        assert!(QuantityExpr::DepthScaledUniform { minimum: 2, maximum_base: 0, per_depth: u64::MAX }.resolve(Some(2)).is_err());
        assert!(QuantityRange::new(0, 1).is_err());
        assert!(QuantityRange::new(2, 1).is_err());
    }

    #[test]
    fn fixed_and_full_width_quantities_have_bounded_deterministic_sampling() {
        let mut state = 91;
        assert_eq!(QuantityRange::new(u64::MAX, u64::MAX).unwrap().sample(&mut state).unwrap(), u64::MAX);
        assert_eq!(state, 91);
        let mut other = state;
        let range = QuantityRange::new(1, u64::MAX).unwrap();
        for _ in 0..32 {
            assert_eq!(range.sample(&mut state).unwrap(), range.sample(&mut other).unwrap());
        }
        assert!(serde_json::from_str::<QuantityRange>(r#"{"minimum":1}"#).is_err());
        assert!(serde_json::from_str::<QuantityRange>(r#"{"minimum":0,"maximum":1}"#).is_err());
    }
}
