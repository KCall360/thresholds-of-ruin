use tor_simulation::health::Health;

#[test]
fn restoring_injury_and_death_does_not_heal_or_revive() {
    let living = Health::from_recorded(20, 7, false).unwrap();
    assert_eq!(living.current(), 13);
    let mut dead = Health::from_recorded(100, 7, true).unwrap();
    assert_eq!(dead.current(), 0);
    assert_eq!(dead.heal(100), 0);
    assert!(Health::from_recorded(0, 0, false).is_err());
    assert!(Health::from_recorded(20, 20, false).is_err());
    assert!(Health::from_recorded(20, 21, false).is_err());
    assert!(Health::from_recorded(0, 7, true).unwrap().dead());
}

#[test]
fn changing_maximum_preserves_injury_without_free_healing() {
    let mut health = Health::new(20);
    assert_eq!(health.damage(7), 7);
    assert_eq!(health.current(), 13);
    health.set_maximum(30);
    assert_eq!(health.current(), 23);
    assert_eq!(health.injury(), 7);
    health.set_maximum(10);
    assert_eq!(health.current(), 3);
    assert_eq!(health.injury(), 7);
    assert_eq!(health.heal(4), 4);
    assert_eq!(health.current(), 7);
    assert_eq!(health.heal(u32::MAX), 3);
    assert_eq!(health.current(), 10);
}

#[test]
fn shrinking_capacity_to_injury_causes_persistent_death() {
    let mut health = Health::new(20);
    health.damage(10);
    health.set_maximum(10);
    assert!(health.dead());
    health.set_maximum(100);
    assert_eq!(health.maximum(), 100);
    assert_eq!(health.current(), 0);
    assert_eq!(health.heal(100), 0);
    assert_eq!(health.injury(), 10);
    assert_eq!(health.damage(100), 0);
}

#[test]
fn lethal_damage_is_bounded_and_cannot_be_undone_by_rederivation() {
    let mut health = Health::new(u32::MAX);
    assert_eq!(health.damage(u32::MAX - 1), u32::MAX - 1);
    assert_eq!(health.current(), 1);
    assert_eq!(health.damage(u32::MAX), 1);
    assert_eq!(health.injury(), u32::MAX);
    assert!(health.dead());
    health.set_maximum(u32::MAX);
    assert_eq!(health.current(), 0);
}

#[test]
fn zero_capacity_and_explicit_death_cannot_revive() {
    let mut zero = Health::new(0);
    assert!(zero.dead());
    zero.set_maximum(10);
    assert_eq!(zero.current(), 0);
    let mut health = Health::new(20);
    health.kill();
    assert!(health.dead());
    assert_eq!(health.current(), 0);
    assert_eq!(health.heal(20), 0);
}

#[test]
fn zero_damage_and_zero_healing_leave_a_living_actor_unchanged() {
    let mut health = Health::new(10);
    assert_eq!(health.damage(0), 0);
    assert_eq!(health.heal(0), 0);
    assert_eq!(health.current(), 10);
    assert!(!health.dead());
}
