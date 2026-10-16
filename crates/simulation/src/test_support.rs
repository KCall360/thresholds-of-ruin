//! Private fixture support for this crate’s unit tests.
use crate::rules::{Ruleset, RulesetDocument, DEFAULT_RULESET_TOML};
use std::sync::Arc;

/// The bundled canonical fixture, loaded through production validation.
pub fn test_ruleset() -> Arc<Ruleset> {
    let document: RulesetDocument =
        toml::from_str(DEFAULT_RULESET_TOML).expect("bundled rules fixture must parse");
    Ruleset::validate(document).expect("bundled rules fixture must validate")
}
