//! Precise reference provenance on the diagnostic path, using the existing parser.
//! Source is already bounded and integrity-checked by the package owner. Successful
//! construction does not retain another syntax tree or compute source locations.
use serde::Deserialize;
use std::ops::Range;
use toml::de::{DeTable, DeValue, ValueDeserializer};

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

/// Locate a scalar reference in one uniquely identified authored declaration.
/// Matching its decoded value prevents a programmatically changed declaration
/// from acquiring a misleading location in an older source document.
pub(super) fn reference_location<'a>(
    text: &'a str,
    collection: &str,
    id: u64,
    fields: &[&str],
    expected: &str,
) -> Option<SourceLocation<'a>> {
    let root = DeValue::Table(DeTable::parse(text).ok()?.into_inner());
    let entries = root.get(collection)?.get_ref().as_array()?;
    let mut matching = entries.iter().filter(|entry| {
        entry.get_ref().get("id").is_some_and(|id_value| {
            u64::deserialize(ValueDeserializer::from(id_value.clone())).ok() == Some(id)
        })
    });
    let declaration = matching.next()?;
    if matching.next().is_some() {
        return None;
    }
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
    use super::*;

    #[test]
    fn references_use_parser_spans_and_unicode_columns_instead_of_matching_text() {
        let text = "# ai = \"missing\"\nactors = [\n {id=2, name=\"missing\"},\n {id=0x3, name=\"Gárd\", ai=\"missing\"},\n]\n";
        let location = reference_location(text, "actors", 3, &["ai"], "missing").unwrap();
        assert_eq!(&text[location.span.clone()], "\"missing\"");
        assert_eq!(location.coordinates(), (4, 27));
        assert_eq!(location.span.start, text.rfind("\"missing\"").unwrap());
    }

    #[test]
    fn reference_provenance_does_not_guess_for_missing_changed_or_ambiguous_declarations() {
        let text = "[[actors]]\nid=3\nai=\"old\"\n";
        assert!(reference_location(text, "actors", 3, &["ai"], "new").is_none());
        assert!(reference_location(text, "actors", 4, &["ai"], "old").is_none());
        assert!(reference_location(text, "actors", 3, &["archetype"], "old").is_none());
        let duplicate = format!("{text}{text}");
        assert!(reference_location(&duplicate, "actors", 3, &["ai"], "old").is_none());
    }

    #[test]
    fn nested_reference_spans_preserve_crlf_byte_offsets() {
        let text = "[[actors]]\r\nid=3\r\nbehavior={ ai=\"guard\" }\r\n";
        let location = reference_location(text, "actors", 3, &["behavior", "ai"], "guard").unwrap();
        assert_eq!(location.coordinates(), (3, 15));
        assert_eq!(&text[location.span], "\"guard\"");
    }
}
