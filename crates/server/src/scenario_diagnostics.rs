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
    Objective,
    Archetype(&'a str),
    Region { file: &'a str, id: u64 },
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
            Self::Objective => format!("{}: objective", source("scenario.toml")),
            Self::Character(id) => format!("{}: character {id}", source("scenario.toml")),
            Self::Region { file, id } => format!("{}: region {id}", source(file)),
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
            Self::Objective => ("Objective", "objective"),
            Self::Character(_) => ("Character", "character"),
            Self::Archetype(_) => ("Archetype", "archetype"),
            Self::Region { .. } => ("Region", "region"),
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
        self.reference_path(
            failure,
            source,
            &[PathSegment::Field(field)],
            ReferenceValue::Text(expected),
        )
    }

    pub fn reference_path(
        self,
        failure: Failure,
        source: Option<&str>,
        path: &[PathSegment<'_>],
        expected: ReferenceValue<'_>,
    ) -> Failure {
        let selection = match self {
            Self::Objective => Selection::Document,
            Self::Region { id, .. } => Selection::Root(id),
            Self::Character(id) => Selection::Declaration("characters", Declaration::Id(id)),
            Self::Archetype(name) => Selection::Declaration("archetypes", Declaration::Name(name)),
            Self::Actor { id, .. } => Selection::Declaration("actors", Declaration::Id(id)),
            Self::Item { id, .. } => Selection::Declaration("items", Declaration::Id(id)),
        };
        let location = source.and_then(|text| value_location(text, selection, path, expected));
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

/// Parser traversal stays typed: array indices never become guessed field names.
#[derive(Clone, Copy)]
pub(super) enum PathSegment<'a> {
    Field(&'a str),
    Index(usize),
}

#[derive(Clone, Copy)]
pub(super) enum ReferenceValue<'a> {
    Text(&'a str),
    Id(u64),
}

#[derive(Clone, Copy)]
enum Selection<'a> {
    Document,
    Root(u64),
    Declaration(&'a str, Declaration<'a>),
}

/// Matching both declaration identity and decoded value rejects stale provenance.
fn value_location<'a>(
    text: &'a str,
    selection: Selection<'_>,
    path: &[PathSegment<'_>],
    expected: ReferenceValue<'_>,
) -> Option<SourceLocation<'a>> {
    let root = DeValue::Table(DeTable::parse(text).ok()?.into_inner());
    let matches_id = |value: &DeValue<'_>, id| {
        value.get("id").is_some_and(|value| {
            u64::deserialize(ValueDeserializer::from(value.clone())).ok() == Some(id)
        })
    };
    let declaration = match selection {
        Selection::Document => &root,
        Selection::Root(id) => {
            if !matches_id(&root, id) {
                return None;
            }
            &root
        }
        Selection::Declaration(collection, declaration) => {
            let collection = root.get(collection)?.get_ref();
            match declaration {
                Declaration::Name(name) => collection.get(name)?,
                Declaration::Id(id) => {
                    let mut matching = collection
                        .as_array()?
                        .iter()
                        .filter(|entry| matches_id(entry.get_ref(), id));
                    let declaration = matching.next()?;
                    if matching.next().is_some() {
                        return None;
                    }
                    declaration
                }
            }
            .get_ref()
        }
    };
    let mut value = declaration;
    let mut selected = None;
    for segment in path {
        let child = match segment {
            PathSegment::Field(field) => value.get(*field)?,
            PathSegment::Index(index) => value.as_array()?.get(*index)?,
        };
        selected = Some(child);
        value = child.get_ref();
    }
    let selected = selected?;
    let matches = match expected {
        ReferenceValue::Text(expected) => value.as_str() == Some(expected),
        ReferenceValue::Id(expected) => {
            u64::deserialize(ValueDeserializer::from(selected.clone())).ok() == Some(expected)
        }
    };
    if !matches {
        return None;
    }
    let span = selected.span();
    text.get(span.clone())?;
    Some(SourceLocation { span, text })
}

#[cfg(test)]
fn reference_location<'a>(
    text: &'a str,
    collection: &str,
    declaration: Declaration<'_>,
    fields: &[&str],
    expected: &str,
) -> Option<SourceLocation<'a>> {
    let path: Vec<_> = fields
        .iter()
        .map(|field| PathSegment::Field(field))
        .collect();
    value_location(
        text,
        Selection::Declaration(collection, declaration),
        &path,
        ReferenceValue::Text(expected),
    )
}

#[cfg(test)]
mod tests {
    use super::super::fail;
    use super::*;

    #[test]
    fn root_array_references_locate_the_requested_occurrence_and_verify_region_identity() {
        let text = "# missing is a decoy\r\nid=2\r\n[generate.items]\r\narchetypes=[\"missing\", \"m\\u0069ssing\"]\r\n";
        let path = [
            PathSegment::Field("generate"),
            PathSegment::Field("items"),
            PathSegment::Field("archetypes"),
            PathSegment::Index(1),
        ];
        let location = value_location(
            text,
            Selection::Root(2),
            &path,
            ReferenceValue::Text("missing"),
        )
        .unwrap();
        assert_eq!(&text[location.span], "\"m\\u0069ssing\"");
        assert!(value_location(
            text,
            Selection::Root(3),
            &path,
            ReferenceValue::Text("missing")
        )
        .is_none());
        assert!(value_location(
            text,
            Selection::Root(2),
            &path,
            ReferenceValue::Text("changed")
        )
        .is_none());
        let out_of_bounds = [
            PathSegment::Field("generate"),
            PathSegment::Field("items"),
            PathSegment::Field("archetypes"),
            PathSegment::Index(2),
        ];
        assert!(value_location(
            text,
            Selection::Root(2),
            &out_of_bounds,
            ReferenceValue::Text("missing")
        )
        .is_none());
    }

    #[test]
    fn numeric_references_match_decoded_ids_and_reject_strings_negative_or_stale_values() {
        let path = [PathSegment::Field("carried_by")];
        let text = "items=[{id=10,carried_by=0x3e7}]";
        let location = value_location(
            text,
            Selection::Declaration("items", Declaration::Id(10)),
            &path,
            ReferenceValue::Id(999),
        )
        .unwrap();
        assert_eq!(&text[location.span], "0x3e7");
        for text in [
            "items=[{id=10,carried_by=998}]",
            "items=[{id=10,carried_by=\"999\"}]",
            "items=[{id=10,carried_by=-1}]",
        ] {
            assert!(value_location(
                text,
                Selection::Declaration("items", Declaration::Id(10)),
                &path,
                ReferenceValue::Id(999)
            )
            .is_none());
        }
    }

    #[test]
    fn root_origin_keeps_semantic_failure_when_source_is_missing_or_stale() {
        let origin = Origin::Region {
            file: "regions/2.toml",
            id: 2,
        };
        for source in [
            None,
            Some("id=3\nzone=\"missing\""),
            Some("id=2\nzone=\"old\""),
        ] {
            let failure =
                origin.reference(fail("Unknown zone reference"), source, "zone", "missing");
            assert_eq!(
                failure.message,
                "regions/2.toml: region 2: Unknown zone reference"
            );
        }
    }

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
