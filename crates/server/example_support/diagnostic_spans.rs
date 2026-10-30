//! Benchmark-only wall-clock aggregation; simulation emits clock-free events.
use std::{cell::RefCell, collections::BTreeMap, time::Instant};
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Timing {
    pub calls: u64,
    pub inclusive_ns: u128,
    pub exclusive_ns: u128,
}
#[derive(Default)]
struct Accumulator {
    stack: Vec<(&'static str, u128, u128)>,
    rows: BTreeMap<&'static str, Timing>,
}
impl Accumulator {
    fn event(&mut self, name: &'static str, enter: bool, now: u128) {
        if enter {
            self.stack.push((name, now, 0));
        } else {
            let (opened, start, children) = self.stack.pop().expect("balanced diagnostic span");
            assert_eq!(opened, name, "LIFO diagnostic spans");
            let elapsed = now - start;
            let row = self.rows.entry(name).or_default();
            row.calls += 1;
            row.inclusive_ns += elapsed;
            row.exclusive_ns += elapsed - children;
            if let Some(parent) = self.stack.last_mut() {
                parent.2 += elapsed;
            }
        }
    }
}
thread_local! {
    static PROFILE: RefCell<(Instant, Accumulator)> = RefCell::new((Instant::now(), Accumulator::default()));
}
pub fn reset() {
    PROFILE.with_borrow_mut(|profile| *profile = (Instant::now(), Accumulator::default()));
}
pub fn event(name: &'static str, enter: bool) {
    PROFILE.with_borrow_mut(|(start, accumulator)| {
        accumulator.event(name, enter, start.elapsed().as_nanos())
    });
}
pub fn results() -> BTreeMap<&'static str, Timing> {
    PROFILE.with_borrow(|(_, accumulator)| {
        assert!(accumulator.stack.is_empty());
        accumulator.rows.clone()
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inclusive_and_exclusive_account_for_nested_recursive_and_sibling_spans() {
        let mut a = Accumulator::default();
        a.event("root", true, 10);
        a.event("child", true, 20);
        a.event("child", true, 25);
        a.event("child", false, 30);
        a.event("child", false, 40);
        a.event("sibling", true, 45);
        a.event("sibling", false, 55);
        a.event("root", false, 70);
        assert!(a.stack.is_empty());
        assert_eq!(
            a.rows["root"],
            Timing {
                calls: 1,
                inclusive_ns: 60,
                exclusive_ns: 30
            }
        );
        assert_eq!(
            a.rows["child"],
            Timing {
                calls: 2,
                inclusive_ns: 25,
                exclusive_ns: 20
            }
        );
        assert_eq!(
            a.rows["sibling"],
            Timing {
                calls: 1,
                inclusive_ns: 10,
                exclusive_ns: 10
            }
        );
        assert_eq!(
            a.rows.values().map(|row| row.exclusive_ns).sum::<u128>(),
            60
        );
    }
}
