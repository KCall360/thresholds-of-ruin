//! Optional clock-free span notifications. The host owns timing and storage.
use std::{cell::Cell, marker::PhantomData, rc::Rc};
type Observer = fn(&'static str, bool);
thread_local! { static OBSERVER: Cell<Option<Observer>> = const { Cell::new(None) }; }
/// Restores the previous observer, including during unwinding. Keep guards in
/// lexical/LIFO order. Observers must not panic or affect game state.
pub struct ObserverGuard(Option<Observer>, PhantomData<Rc<()>>);
impl Drop for ObserverGuard {
    fn drop(&mut self) {
        OBSERVER.set(self.0);
    }
}
/// Install a thread-local observer: true means enter, false means exit.
pub fn observe(observer: Observer) -> ObserverGuard {
    ObserverGuard(OBSERVER.replace(Some(observer)), PhantomData)
}
/// Clock-free, thread-bound scope; disabled spans read no clock or game state.
pub struct Span(Option<Observer>, &'static str, PhantomData<Rc<()>>);
pub fn span(name: &'static str) -> Span {
    let observer = OBSERVER.get();
    if let Some(observer) = observer {
        observer(name, true);
    }
    Span(observer, name, PhantomData)
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some(observer) = self.0 {
            observer(self.1, false);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    thread_local! { static EVENTS: RefCell<Vec<(&'static str, bool)>> = const { RefCell::new(Vec::new()) }; }
    fn record(name: &'static str, enter: bool) {
        EVENTS.with_borrow_mut(|events| events.push((name, enter)));
    }
    #[test]
    fn nested_order_disabled_and_thread_isolation() {
        EVENTS.with_borrow_mut(Vec::clear);
        drop(span("disabled"));
        {
            let _observer = observe(record);
            let _outer = span("outer");
            drop(span("inner"));
            std::thread::spawn(|| {
                drop(span("other_thread"));
                EVENTS.with_borrow(|events| assert!(events.is_empty()));
            })
            .join()
            .unwrap();
        }
        drop(span("disabled_again"));
        EVENTS.with_borrow(|events| {
            assert_eq!(
                events,
                &[
                    ("outer", true),
                    ("inner", true),
                    ("inner", false),
                    ("outer", false)
                ]
            )
        });
    }
    #[test]
    fn unwind_closes_scopes_and_restores_previous_observer() {
        EVENTS.with_borrow_mut(Vec::clear);
        let outer = observe(record);
        let result = std::panic::catch_unwind(|| {
            let _inner = observe(record);
            let _span = span("unwind");
            panic!("intentional diagnostic unwind");
        });
        assert!(result.is_err());
        drop(span("restored"));
        drop(outer);
        drop(span("disabled"));
        EVENTS.with_borrow(|events| {
            assert_eq!(
                events,
                &[
                    ("unwind", true),
                    ("unwind", false),
                    ("restored", true),
                    ("restored", false)
                ]
            )
        });
    }
}
