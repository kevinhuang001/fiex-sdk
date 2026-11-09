mod host;
mod state;

use crate::{context::ContextInner, runtime::session::Session, tasks::TaskManager};
use std::{sync::Arc, sync::Mutex, sync::atomic::AtomicBool};

pub(super) use state::State;

pub(super) struct Mock {
    pub(super) state: Mutex<State>,
}

impl Mock {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
        }
    }

    pub(super) fn with_bytes(bytes: Vec<u8>) -> Self {
        Self {
            state: Mutex::new(State {
                bytes,
                ..State::default()
            }),
        }
    }

    pub(super) fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    pub(super) fn count(&self, method: &str) -> usize {
        self.state()
            .calls
            .iter()
            .filter(|call| call.as_str() == method)
            .count()
    }
}

fn write_staged(state: &mut State, bytes: &[u8]) {
    if state.staged.len() < state.position + bytes.len() {
        state.staged.resize(state.position + bytes.len(), 0);
    }
    state.staged[state.position..state.position + bytes.len()].copy_from_slice(bytes);
    state.position += bytes.len();
}

pub(super) fn context(link: Arc<Mock>) -> ContextInner {
    ContextInner::new(
        link,
        Arc::new(TaskManager::new(4)),
        Session::Operation(7),
        Arc::new(AtomicBool::new(false)),
    )
}

pub(super) fn reference() -> crate::ResourceRef {
    crate::ResourceRef {
        mount_id: "local".into(),
        node_id: "opaque-file".into(),
    }
}
