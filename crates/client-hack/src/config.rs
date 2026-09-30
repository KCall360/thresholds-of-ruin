//! Player options from one TOML file. Omitted `--config` uses these defaults.
use serde::Deserialize;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Click {
    #[default]
    Travel,
    Look,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BumpAttacks {
    #[default]
    Hostile,
    Any,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionConfig {
    pub autopickup: bool,
    pub click: Click,
    pub bump_attacks: BumpAttacks,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            autopickup: true,
            click: Click::Travel,
            bump_attacks: BumpAttacks::Hostile,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default = "default_true")]
    autopickup: bool,
    #[serde(default)]
    click: Click,
    #[serde(default)]
    bump_attacks: BumpAttacks,
}

impl From<FileConfig> for SessionConfig {
    fn from(file: FileConfig) -> Self {
        Self {
            autopickup: file.autopickup,
            click: file.click,
            bump_attacks: file.bump_attacks,
        }
    }
}

pub fn parse_config(text: &str) -> Result<SessionConfig, String> {
    if text.trim().is_empty() {
        return Ok(SessionConfig::default());
    }
    let file: FileConfig = toml::from_str(text).map_err(|error| error.to_string())?;
    Ok(file.into())
}

pub fn load_config(path: &Path) -> Result<SessionConfig, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    parse_config(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_partial_tables_use_defaults() {
        let defaults = SessionConfig::default();
        assert!(defaults.autopickup);
        assert_eq!(defaults.click, Click::Travel);
        assert_eq!(defaults.bump_attacks, BumpAttacks::Hostile);
        assert_eq!(parse_config("").unwrap(), defaults);
        assert_eq!(parse_config("   \n# comment only\n").unwrap(), defaults);
        assert_eq!(parse_config("# keep\n").unwrap(), defaults);
        assert_eq!(
            parse_config("autopickup = false\n").unwrap().click,
            Click::Travel
        );
        assert!(parse_config("click = \"look\"\n").unwrap().autopickup);
    }

    #[test]
    fn each_legal_value_parses() {
        let config =
            parse_config("autopickup = false\nclick = \"look\"\nbump_attacks = \"any\"\n").unwrap();
        assert!(!config.autopickup);
        assert_eq!(config.click, Click::Look);
        assert_eq!(config.bump_attacks, BumpAttacks::Any);
        assert_eq!(
            parse_config("bump_attacks = \"off\"\n")
                .unwrap()
                .bump_attacks,
            BumpAttacks::Off
        );
        assert_eq!(
            parse_config("bump_attacks = \"hostile\"\n")
                .unwrap()
                .bump_attacks,
            BumpAttacks::Hostile
        );
        assert_eq!(
            parse_config("click = \"travel\"\n").unwrap().click,
            Click::Travel
        );
    }

    #[test]
    fn unknown_key_names_the_key() {
        let error = parse_config("fly = true\n").unwrap_err();
        assert!(error.contains("fly"), "{error}");
    }

    #[test]
    fn illegal_click_names_the_key() {
        let error = parse_config("click = \"fly\"\n").unwrap_err();
        assert!(error.contains("click"), "{error}");
    }

    #[test]
    fn illegal_bump_attacks_names_the_key() {
        let error = parse_config("bump_attacks = \"fly\"\n").unwrap_err();
        assert!(error.contains("bump_attacks"), "{error}");
    }

    #[test]
    fn missing_file_fails() {
        let error = load_config(Path::new("missing-tor-client-config.toml")).unwrap_err();
        assert!(error.contains("missing-tor-client-config.toml"), "{error}");
    }
}
