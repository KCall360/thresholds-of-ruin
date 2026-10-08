//! Precise reference provenance on the diagnostic path, using the existing parser.
//! Source is already bounded and integrity-checked by the package owner. Successful
//! construction does not retain another syntax tree or compute source locations.
use super::{in_declaration, Failure};
use serde::Deserialize;
use std::ops::Range;
use toml::de::{DeTable, DeValue, ValueDeserializer};

#[derive(Clone, Copy)]
pub(super) enum Origin<'a> {
    Character(u64),
    Archetype(&'a str),
    Actor { file: &'a str, region: u64, id: u64 },
    Item { file: &'a str, region: u64, id: u64 },
}

impl Origin<'_> {
    pub fn context(self, failure: Failure) -> Failure {
        self.context_at(failure, None)
    }

    fn context_at(self, failure: Failure, coordinates: Option<(usize, usize)>) -> Failure {
        let source = |file: &str| match coordinates {
            Some((line, column)) => format!("{file}:{line}:{column}"),
            None => file.into(),
        };
        let declaration = match self {
            Self::Archetype(name) => format!("{}: archetype {name:?}", source("scenario.toml")),
            Self::Character(id) => format!("{}: character {id}", source("scenario.toml")),
            Self::Actor { file, region, id } => {
                format!("{}: region {region}, actor {id}", source(file))
            }
            Self::Item { file, region, id } => {
                format!("{}: region {region}, item {id}", source(file))
            }
        };
        in_declaration(failure, declaration)
    }

    pub(super) fn names(self) -> (&'static str, &'static str) {
        match self {
            Self::Character(_) => ("Character", "character"),
            Self::Archetype(_) => ("Archetype", "archetype"),
            Self::Actor { .. } => ("Actor", "actor"),
            Self::Item { .. } => ("Item", "item"),
        }
    }

    pub fn reference(
        self,
        failure: Failure,
        source: Option<&str>,
        field: &str,
        expected: &str,
    ) -> Failure {
        let (collection, declaration) = match self {
            Self::Character(id) => ("characters", Declaration::Id(id)),
            Self::Archetype(name) => ("archetypes", Declaration::Name(name)),
            Self::Actor { id, .. } => ("actors", Declaration::Id(id)),
            Self::Item { id, .. } => ("items", Declaration::Id(id)),
        };
        let location = source
            .and_then(|text| reference_location(text, collection, declaration, &[field], expected));
        self.context_at(failure, location.map(|location| location.coordinates()))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SourceLocation<'a> {
    pub span: Range<usize>,
    text: &'a str,
}

impl SourceLocation<'_> {
    pub fn coordinates(&self) -> (usize, usize) {
        let prefix = &self.text[..self.span.start];
        (
            prefix.bytes().filter(|byte| *byte == b'\n').count() + 1,
            prefix.rsplit('\n').next().unwrap().chars().count() + 1,
        )
    }
}

/// The authored identity used to select an array entry or a named table.
#[derive(Clone, Copy)]
pub(super) enum Declaration<'a> {
    Id(u64),
    Name(&'a str),
}

/// Locate a scalar reference in one uniquely identified authored declaration.
/// Matching its decoded value prevents a programmatically changed declaration
/// from acquiring a misleading location in an older source document.
pub(super) fn reference_location<'a>(
    text: &'a str,
    collection: &str,
    declaration: Declaration<'_>,
    fields: &[&str],
    expected: &str,
) -> Option<SourceLocation<'a>> {
    let root = DeValue::Table(DeTable::parse(text).ok()?.into_inner());
    let collection = root.get(collection)?.get_ref();
    let declaration = match declaration {
        Declaration::Name(name) => collection.get(name)?,
        Declaration::Id(id) => {
            let mut matching = collection.as_array()?.iter().filter(|entry| {
                entry.get_ref().get("id").is_some_and(|id_value| {
                    u64::deserialize(ValueDeserializer::from(id_value.clone())).ok() == Some(id)
                })
            });
            let declaration = matching.next()?;
            if matching.next().is_some() {
                return None;
            }
            declaration
        }
    };
    let mut value = declaration;
    for field in fields {
        value = value.get_ref().get(*field)?;
    }
    if value.get_ref().as_str()? != expected {
        return None;
    }
    let span = value.span();
    text.get(span.clone())?;
    Some(SourceLocation { span, text })
}

#[cfg(test)]
mod tests {
    use super::super::fail;
    use super::*;

    #[test]
    fn named_references_follow_the_table_key_and_decoded_value_with_unicode_columns() {
        let text = "# appearance_pool = \"missing\"\narchetypes = { healing = { name = \"Gárd\", appearance_pool = \"m\\u0069ssing\" }, poison = { appearance_pool = \"missing\" } }\n";
        let location = reference_location(
            text,
            "archetypes",
            Declaration::Name("healing"),
            &["appearance_pool"],
            "missing",
        )
        .unwrap();
        assert_eq!(location.coordinates(), (2, 61));
        assert_eq!(&text[location.span], "\"m\\u0069ssing\"");
    }

    #[test]
    fn named_references_do_not_guess_for_changed_missing_or_different_table_keys() {
        let text = "[archetypes.\"healing.dot\"]\r\nappearance_pool=\"old\"\r\n";
        for (name, expected) in [
            ("healing.dot", "new"),
            ("healing", "old"),
            ("absent", "old"),
        ] {
            assert!(reference_location(
                text,
                "archetypes",
                Declaration::Name(name),
                &["appearance_pool"],
                expected
            )
            .is_none());
        }
        let location = reference_location(
            text,
            "archetypes",
            Declaration::Name("healing.dot"),
            &["appearance_pool"],
            "old",
        )
        .unwrap();
        assert_eq!(location.coordinates(), (2, 17));
    }

    #[test]
    fn named_origin_retains_context_when_source_is_missing_or_changed() {
        for source in [
            None,
            Some("[archetypes.healing]\nappearance_pool=\"old\"\n"),
        ] {
            let failure = Origin::Archetype("healing").reference(
                fail("Unknown appearance pool"),
                source,
                "appearance_pool",
                "new",
            );
            assert_eq!(
                failure.message,
                "scenario.toml: archetype \"healing\": Unknown appearance pool"
            );
        }
    }

    #[test]
    fn references_use_parser_spans_and_unicode_columns_instead_of_matching_text() {
        let text = "# ai = \"missing\"\nactors = [\n {id=2, name=\"missing\"},\n {id=0x3, name=\"Gárd\", ai=\"missing\"},\n]\n";
        let location =
            reference_location(text, "actors", Declaration::Id(3), &["ai"], "missing").unwrap();
        assert_eq!(&text[location.span.clone()], "\"missing\"");
        assert_eq!(location.coordinates(), (4, 27));
        assert_eq!(location.span.start, text.rfind("\"missing\"").unwrap());
    }

    #[test]
    fn reference_provenance_does_not_guess_for_missing_changed_or_ambiguous_declarations() {
        let text = "[[actors]]\nid=3\nai=\"old\"\n";
        assert!(reference_location(text, "actors", Declaration::Id(3), &["ai"], "new").is_none());
        assert!(reference_location(text, "actors", Declaration::Id(4), &["ai"], "old").is_none());
        assert!(
            reference_location(text, "actors", Declaration::Id(3), &["archetype"], "old").is_none()
        );
        let duplicate = format!("{text}{text}");
        assert!(
            reference_location(&duplicate, "actors", Declaration::Id(3), &["ai"], "old").is_none()
        );
    }

    #[test]
    fn nested_reference_spans_preserve_crlf_byte_offsets() {
        let text = "[[actors]]\r\nid=3\r\nbehavior={ ai=\"guard\" }\r\n";
        let location = reference_location(
            text,
            "actors",
            Declaration::Id(3),
            &["behavior", "ai"],
            "guard",
        )
        .unwrap();
        assert_eq!(location.coordinates(), (3, 15));
        assert_eq!(&text[location.span], "\"guard\"");
    }

    #[test]
    fn reference_context_locates_each_declaration_kind_and_preserves_the_failure() {
        for (origin, collection, context) in [
            (
                Origin::Character(3),
                "characters",
                "scenario.toml:3:11: character 3",
            ),
            (
                Origin::Actor {
                    file: "regions/2.toml",
                    region: 2,
                    id: 3,
                },
                "actors",
                "regions/2.toml:3:11: region 2, actor 3",
            ),
            (
                Origin::Item {
                    file: "regions/2.toml",
                    region: 2,
                    id: 3,
                },
                "items",
                "regions/2.toml:3:11: region 2, item 3",
            ),
        ] {
            let text = format!("[[{collection}]]\nid=3\narchetype=\"missing\"\n");
            let original = fail("Unknown archetype missing");
            let code = original.code;
            let failure = origin.reference(original, Some(&text), "archetype", "missing");
            assert_eq!(failure.code, code);
            assert_eq!(
                failure.message,
                format!("{context}: Unknown archetype missing")
            );
        }
    }

    #[test]
    fn unavailable_or_changed_source_keeps_declaration_context_without_coordinates() {
        let origin = Origin::Character(3);
        for source in [None, Some("[[characters]]\nid=3\nai=\"old\"\n")] {
            let failure = origin.reference(fail("Unknown AI profile"), source, "ai", "new");
            assert_eq!(
                failure.message,
                "scenario.toml: character 3: Unknown AI profile"
            );
        }
    }
}
