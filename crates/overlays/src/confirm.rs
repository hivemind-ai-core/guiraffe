//! Single-threaded awaitable modal result. Dropping its sender resolves cancellation.
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Default)]
struct State {
    result: Option<bool>,
    waker: Option<Waker>,
}
/// Resolves `true` for confirmation and `false` for any cancellation or window destruction.
pub struct ConfirmResult(Rc<RefCell<State>>);
pub(crate) struct ConfirmSender(Rc<RefCell<State>>);
pub(crate) fn channel() -> (ConfirmSender, ConfirmResult) {
    let state = Rc::new(RefCell::new(State::default()));
    (ConfirmSender(state.clone()), ConfirmResult(state))
}
impl ConfirmSender {
    pub fn resolve(&self, result: bool) {
        let waker = {
            let mut state = self.0.borrow_mut();
            if state.result.is_some() {
                return;
            }
            state.result = Some(result);
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}
impl Drop for ConfirmSender {
    fn drop(&mut self) {
        self.resolve(false);
    }
}
impl Future for ConfirmResult {
    type Output = bool;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        let mut state = self.0.borrow_mut();
        match state.result {
            Some(result) => Poll::Ready(result),
            None => {
                state.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::task::Wake;
    struct Counter(AtomicUsize);
    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[test]
    fn wakes_and_first_resolution_wins() {
        let (sender, mut result) = channel();
        let counter = Arc::new(Counter(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut cx = Context::from_waker(&waker);
        assert_eq!(Pin::new(&mut result).poll(&mut cx), Poll::Pending);
        sender.resolve(true);
        sender.resolve(false);
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);
        assert_eq!(Pin::new(&mut result).poll(&mut cx), Poll::Ready(true));
    }
    #[test]
    fn sender_drop_cancels() {
        let (sender, mut result) = channel();
        drop(sender);
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(Pin::new(&mut result).poll(&mut cx), Poll::Ready(false));
    }
}
