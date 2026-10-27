use super::*;

fn row(owner: HdOwner, seed: u64) -> OwnedHitDie {
    OwnedHitDie::new(
        owner,
        HitDie::new(HdSource::Racial, seed),
        vec![],
        None,
        &crate::test_support::test_ruleset(),
    )
    .unwrap()
}
fn body(form: &str, ordinal: u16) -> HdOwner {
    HdOwner::Body {
        form: BodyFormId::new(form).unwrap(),
        ordinal,
    }
}
fn personal(id: u64) -> HdOwner {
    HdOwner::Personal {
        id: PersonalHdId(NonZeroU64::new(id).unwrap()),
    }
}
#[test]
fn package_size_changes_neither_personal_identity_nor_seed_or_budget() {
    let small = [row(body("human", 0), 1)];
    let large = [row(body("troll", 0), 2), row(body("troll", 1), 3)];
    let person = [row(personal(7), 99)];
    let before = Composition::new(&small, &person).unwrap();
    let after = Composition::new(&large, &person).unwrap();
    assert_eq!((before.combat_count(), after.combat_count()), (2, 3));
    assert_eq!((before.personal_count(), after.personal_count()), (1, 1));
    assert_eq!(before.rows().last(), after.rows().last());
    assert_eq!(after.rows().last().unwrap().hit_die().health_seed(), 99);
    let rules = crate::test_support::test_ruleset();
    assert_eq!(before.personal_unspent_points(&rules).unwrap(), 1);
    assert_eq!(after.personal_unspent_points(&rules).unwrap(), 1);
    assert_eq!(after.next_drain_owner(), &personal(7));
    assert!(!person[0].owner().maximizes_health());
    assert!(large[0].owner().maximizes_health());
    assert!(!large[1].owner().maximizes_health());
}
#[test]
fn zero_personal_hd_is_valid_and_drain_falls_back_to_package() {
    let package = [row(body("troll", 0), 1), row(body("troll", 1), 2)];
    let composition = Composition::new(&package, &[]).unwrap();
    assert_eq!(composition.next_drain_owner(), &body("troll", 1));
    assert_eq!(composition.personal_count(), 0);
}
#[test]
fn composition_rejects_duplicate_reordered_and_wrong_domain_owners() {
    let package = [row(body("human", 0), 1)];
    let duplicate = [row(personal(2), 2), row(personal(2), 3)];
    let reordered = [row(personal(3), 2), row(personal(2), 3)];
    assert!(Composition::new(&package, &duplicate).is_err());
    assert!(Composition::new(&package, &reordered).is_err());
    assert!(Composition::new(&package, &package).is_err());
    assert!(Composition::new(&[], &[]).is_err());
    let mixed = [row(body("human", 0), 1), row(body("troll", 1), 2)];
    assert!(Composition::new(&mixed, &[]).is_err());
}
#[test]
fn package_cannot_own_personal_attribute_opportunities() {
    assert!(OwnedHitDie::new(
        body("human", 0),
        HitDie::new(HdSource::Racial, 1),
        vec![],
        Some(Attribute::Strength),
        &crate::test_support::test_ruleset()
    )
    .is_err());
}
#[test]
fn funding_owner_cannot_be_forged_or_serial_reused() {
    let rules = crate::test_support::test_ruleset();
    let purchase = AdvancementPurchase {
        id: Some(PurchaseId {
            owner: personal(2),
            serial: NonZeroU64::new(1).unwrap(),
        }),
        pool: PointPoolId::new("advancement").unwrap(),
        selection: PurchaseSelection::SkillRank(crate::attributes::Skill::HeavyWeaponry),
    };
    assert_eq!(
        OwnedHitDie::new(
            personal(1),
            HitDie::new(HdSource::Racial, 1),
            vec![purchase],
            None,
            &rules
        ),
        Err(BuildError::InvalidOwnership)
    );
}
