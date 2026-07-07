#[derive(Clone, Default)]
pub struct EventHub {
    inner: std::sync::Arc<EventHubInner>,
}

#[derive(Default)]
struct EventHubInner {
    state: std::sync::Mutex<EventHubState>,
    changed: std::sync::Condvar,
}

#[derive(Default)]
struct EventHubState {
    next_sequence: u64,
    events: Vec<(u64, crate::api::schema::EventEnvelope)>,
}

impl EventHub {
    const MAX_EVENTS: usize = 512;

    pub fn push(&self, event: crate::api::schema::EventEnvelope) {
        let Ok(mut state) = self.inner.state.lock() else {
            return;
        };
        state.next_sequence += 1;
        let sequence = state.next_sequence;
        state.events.push((sequence, event));
        let overflow = state.events.len().saturating_sub(Self::MAX_EVENTS);
        if overflow > 0 {
            state.events.drain(0..overflow);
        }
        self.inner.changed.notify_all();
    }

    pub fn events_after(&self, sequence: u64) -> Vec<(u64, crate::api::schema::EventEnvelope)> {
        let Ok(state) = self.inner.state.lock() else {
            return Vec::new();
        };
        state
            .events
            .iter()
            .filter(|(event_sequence, _)| *event_sequence > sequence)
            .cloned()
            .collect()
    }

    pub fn current_sequence(&self) -> u64 {
        let Ok(state) = self.inner.state.lock() else {
            return 0;
        };
        state.next_sequence
    }

    pub fn wait_for_change_after(&self, sequence: u64, timeout: std::time::Duration) -> bool {
        let Ok(state) = self.inner.state.lock() else {
            return false;
        };
        if state.next_sequence > sequence {
            return true;
        }
        self.inner
            .changed
            .wait_timeout_while(state, timeout, |state| state.next_sequence <= sequence)
            .ok()
            .is_some_and(|(state, _)| state.next_sequence > sequence)
    }
}
