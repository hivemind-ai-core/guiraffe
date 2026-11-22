//! An in-memory [`NotificationBackend`] for tests: records every delivery and lets the test
//! play the notification server (respond with clicks, actions or closes, or fail deliveries).
//!
//! ```
//! use guiraffe_overlays::desktop::mock::MockBackend;
//! let mock = MockBackend::new();
//! assert!(mock.deliveries().is_empty());
//! ```
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use futures::channel::oneshot;

use super::backend::{
    BoxFuture, Capabilities, Delivery, DesktopError, NotificationBackend, Response, Shown,
};

#[derive(Default)]
struct State {
    next_id: u32,
    deliveries: Vec<Delivery>,
    closed: Vec<u32>,
    pending: HashMap<u32, oneshot::Sender<Option<Response>>>,
    fail_next: Option<DesktopError>,
}

/// Cheap to clone; clones share state, so keep one in the test after installing another.
#[derive(Clone)]
pub struct MockBackend {
    state: Arc<Mutex<State>>,
    capabilities: Capabilities,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBackend {
    /// A backend with every capability.
    pub fn new() -> Self {
        Self::with_capabilities(Capabilities::ALL)
    }
    pub fn with_capabilities(capabilities: Capabilities) -> Self {
        Self {
            state: Arc::default(),
            capabilities,
        }
    }
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Every delivery so far, in order (updates included).
    pub fn deliveries(&self) -> Vec<Delivery> {
        self.state().deliveries.clone()
    }
    /// Platform ids closed through [`NotificationBackend::close`].
    pub fn closed(&self) -> Vec<u32> {
        self.state().closed.clone()
    }
    /// Platform ids still waiting for a response.
    pub fn open(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.state().pending.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
    /// Make the next delivery fail with `error`.
    pub fn fail_next(&self, error: DesktopError) {
        self.state().fail_next = Some(error);
    }
    /// Play the server: resolve the notification's outcome. Returns false if it isn't open.
    pub fn respond(&self, platform_id: u32, response: Response) -> bool {
        let sender = self.state().pending.remove(&platform_id);
        sender.is_some_and(|s| s.send(Some(response)).is_ok())
    }
    /// Play a server that goes away without reporting anything.
    pub fn detach(&self, platform_id: u32) -> bool {
        let sender = self.state().pending.remove(&platform_id);
        sender.is_some_and(|s| s.send(None).is_ok())
    }
}

impl NotificationBackend for MockBackend {
    fn name(&self) -> &'static str {
        "mock"
    }
    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
    fn show(&self, delivery: Delivery) -> BoxFuture<Result<Shown, DesktopError>> {
        let mut state = self.state();
        if let Some(error) = state.fail_next.take() {
            return Box::pin(async move { Err(error) });
        }
        let replace = self.capabilities.replace;
        let platform_id = match delivery.replaces {
            Some(id) if replace => id,
            _ => {
                state.next_id += 1;
                state.next_id
            }
        };
        state.deliveries.push(delivery);
        let (tx, rx) = oneshot::channel();
        // Replacing drops the previous listener, like a server re-using the id.
        state.pending.insert(platform_id, tx);
        Box::pin(async move {
            Ok(Shown {
                platform_id,
                outcome: Box::pin(async move { rx.await.ok().flatten() }),
            })
        })
    }
    fn close(&self, platform_id: u32) -> BoxFuture<Result<(), DesktopError>> {
        let mut state = self.state();
        state.closed.push(platform_id);
        let known = state.pending.remove(&platform_id).is_some();
        Box::pin(async move {
            if known {
                Ok(())
            } else {
                Err(DesktopError::Unknown(platform_id.into()))
            }
        })
    }
}
