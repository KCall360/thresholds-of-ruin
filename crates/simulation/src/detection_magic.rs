//! Staged separate predicate. Existing Item::magical/theft remains untouched.
use crate::armor::ItemCondition;
fn detectable(authored: bool, condition: &ItemCondition) -> bool {
    authored
        || condition.cursed
        || condition.protected
        || condition.armor_enchantment != 0
        || condition.hit_enchantment != 0
        || condition.damage_enchantment != 0
}
impl crate::Item {
    pub(crate) fn detection_magic(&self) -> bool {
        detectable(self.spec.magical, &self.condition)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mundane_and_corrosion_only_are_negative() {
        assert!(!detectable(false, &ItemCondition::default()));
        assert!(!detectable(
            false,
            &ItemCondition {
                corrosion: 3,
                armor_penalty: 3,
                ..ItemCondition::default()
            }
        ));
    }
    #[test]
    fn each_magic_contribution_is_independently_positive() {
        assert!(detectable(true, &ItemCondition::default()));
        for condition in [
            ItemCondition {
                cursed: true,
                ..ItemCondition::default()
            },
            ItemCondition {
                protected: true,
                ..ItemCondition::default()
            },
            ItemCondition {
                armor_enchantment: 1,
                ..ItemCondition::default()
            },
            ItemCondition {
                hit_enchantment: 1,
                ..ItemCondition::default()
            },
            ItemCondition {
                hit_enchantment: -1,
                ..ItemCondition::default()
            },
            ItemCondition {
                damage_enchantment: 1,
                ..ItemCondition::default()
            },
            ItemCondition {
                damage_enchantment: -1,
                ..ItemCondition::default()
            },
        ] {
            assert!(detectable(false, &condition));
        }
    }
    #[test]
    fn offsetting_contributions_never_cancel_magic() {
        assert!(detectable(
            false,
            &ItemCondition {
                armor_penalty: 2,
                armor_enchantment: 2,
                hit_enchantment: 3,
                damage_enchantment: -3,
                ..ItemCondition::default()
            }
        ));
    }
}
