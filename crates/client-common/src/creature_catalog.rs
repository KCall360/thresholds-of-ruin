//! Read-only named creature discovery shared by Text and ASCII.
use tor_protocol::{CreatureCatalogPage, InvalidCreatureCatalog};

pub fn lines(page: &CreatureCatalogPage) -> Result<Vec<String>, InvalidCreatureCatalog> {
    page.validate()?;
    let mut rows = vec![match page.depth {
        Some(depth) => format!("Creature catalog (depth {depth})"),
        None => "Creature catalog (authored base; no floor depth)".into(),
    }];
    if page.entries.is_empty() {
        rows.push("No creatures on this page.".into());
    }
    rows.extend(
        page.entries
            .iter()
            .map(|entry| format!("{}: {} - {}", entry.selector, entry.name, entry.details)),
    );
    rows.push("Spawn: wizard creature spawn <selector> <region> <x> <y> <z> [depth <N>]".into());
    rows.push("List: wizard creature list [depth <N>] [after <selector>]".into());
    if let Some(next) = &page.next {
        let depth = page
            .depth
            .map(|depth| format!(" depth {depth}"))
            .unwrap_or_default();
        rows.push(format!("Next: wizard creature list{depth} after {next}"));
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use tor_protocol::*;

    #[test]
    fn creature_catalog_rows_identify_depth_selectors_and_exact_followup_commands() {
        let mut page = CreatureCatalogPage {
            depth: Some(27),
            entries: vec![CreatureCatalogEntry {
                selector: "rogue:vampire".into(),
                name: "Vampire".into(),
                details: "Racial growth; finite birth stock".into(),
            }],
            next: Some("rogue:vampire".into()),
        };
        let rows = super::lines(&page).unwrap();
        assert_eq!(rows[0], "Creature catalog (depth 27)");
        assert!(rows.contains(&"rogue:vampire: Vampire - Racial growth; finite birth stock".into()));
        assert!(rows.contains(
            &"Spawn: wizard creature spawn <selector> <region> <x> <y> <z> [depth <N>]".into()
        ));
        assert!(rows.contains(&"List: wizard creature list [depth <N>] [after <selector>]".into()));
        assert!(rows.contains(&"Next: wizard creature list depth 27 after rogue:vampire".into()));
        page.depth = None;
        let rows = super::lines(&page).unwrap();
        assert_eq!(rows[0], "Creature catalog (authored base; no floor depth)");
        assert!(rows.contains(&"Next: wizard creature list after rogue:vampire".into()));
        page.entries.clear();
        page.next = None;
        assert!(super::lines(&page)
            .unwrap()
            .contains(&"No creatures on this page.".into()));
        page.depth = Some(0);
        assert!(super::lines(&page).is_err());
    }
}
