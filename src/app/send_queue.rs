//! Composition-aware queueing for API-originated pane sends.
//!
//! Human keystrokes and API sends share one PTY per pane. Raw API writes can
//! splice into a half-typed human message in the pane's composer, so API
//! sends (`pane.send_text`, `pane.send_keys`, `pane.send_input`,
//! `agent.send`) are queued per terminal and flushed only when the send gate
//! is open (see [`crate::terminal::TerminalState::send_gate_open`]): no
//! recent human keystrokes and no human-authored text sitting in the visible
//! composer. Human input is never queued, and callers can bypass the queue
//! with the `now` param (`--now` in the CLI).

use std::time::{Duration, Instant};

use bytes::Bytes;
use tracing::warn;

use crate::app::api_helpers::{encode_api_keys, encode_api_text};
use crate::app::App;
use crate::detect::detect_composer_state;
use crate::layout::PaneId;
use crate::terminal::state::QueuedSend;
use crate::terminal::TerminalId;

pub(crate) const SEND_QUEUE_FLUSH_INTERVAL: Duration = Duration::from_millis(500);

/// Outcome of enqueueing an API-originated send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SendDisposition {
    /// The send was written to the PTY immediately (gate open, queue empty).
    Delivered,
    /// The send is held in the pane's queue.
    Queued { depth: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SendQueueError {
    PaneNotFound,
    InvalidKey(String),
    QueueFull,
}

#[derive(Debug, Clone, Copy, Default)]
struct FlushOutcome {
    delivered: usize,
    remaining: usize,
}

impl App {
    /// Queue an API-originated send for a pane and attempt to flush it
    /// immediately. When the gate is open and nothing else is queued this is
    /// equivalent to the historical direct write.
    pub(crate) fn enqueue_pane_send(
        &mut self,
        ws_idx: usize,
        pane_id: PaneId,
        text: String,
        encode_text: bool,
        keys: Vec<String>,
        now: Instant,
    ) -> Result<SendDisposition, SendQueueError> {
        {
            let Some(runtime) = self.lookup_runtime_sender(ws_idx, pane_id) else {
                return Err(SendQueueError::PaneNotFound);
            };
            if let Err(key) = encode_api_keys(runtime, &keys) {
                return Err(SendQueueError::InvalidKey(key));
            }
        }
        let Some(terminal_id) = self.state.terminal_id_for_pane(ws_idx, pane_id) else {
            return Err(SendQueueError::PaneNotFound);
        };
        let Some(terminal) = self.state.terminals.get_mut(&terminal_id) else {
            return Err(SendQueueError::PaneNotFound);
        };
        terminal
            .enqueue_send(QueuedSend {
                text,
                encode_text,
                keys,
                enqueued_at: now,
            })
            .map_err(|()| SendQueueError::QueueFull)?;

        let outcome = self.flush_terminal_send_queue(ws_idx, pane_id, terminal_id, now);
        if outcome.remaining > 0 {
            self.schedule_send_queue_flush(now);
            if outcome.delivered == 0 {
                self.emit_send_queue_event(
                    ws_idx,
                    pane_id,
                    crate::api::schema::EventKind::PaneSendQueued,
                    0,
                    outcome.remaining,
                );
            }
            Ok(SendDisposition::Queued {
                depth: outcome.remaining,
            })
        } else {
            Ok(SendDisposition::Delivered)
        }
    }

    /// Record human-originated input for a terminal by id (attached-client
    /// byte input) so queued sends hold while a human is typing.
    pub(crate) fn note_terminal_human_input(&mut self, terminal_id: TerminalId, now: Instant) {
        if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
            terminal.note_human_input(now);
        }
    }

    /// Record human-originated input to a pane (TUI keystrokes, attached
    /// client bytes, paste) so queued sends hold while a human is typing.
    pub(crate) fn note_pane_human_input(&mut self, ws_idx: usize, pane_id: PaneId, now: Instant) {
        let Some(terminal_id) = self.state.terminal_id_for_pane(ws_idx, pane_id) else {
            return;
        };
        if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
            terminal.note_human_input(now);
        }
    }

    /// Bookkeeping for a `now`-bypass write: composer text written that way
    /// is agent-authored, and callers get the current queue depth back.
    pub(crate) fn note_pane_agent_send_now(
        &mut self,
        ws_idx: usize,
        pane_id: PaneId,
        now: Instant,
    ) -> u32 {
        let Some(terminal_id) = self.state.terminal_id_for_pane(ws_idx, pane_id) else {
            return 0;
        };
        let Some(terminal) = self.state.terminals.get_mut(&terminal_id) else {
            return 0;
        };
        terminal.note_agent_send(now);
        terminal.send_queue_depth() as u32
    }

    /// Attempt to flush every terminal's send queue. Returns true when
    /// anything was delivered. Reschedules the retry deadline while any
    /// queue stays non-empty.
    pub(crate) fn flush_send_queues(&mut self, now: Instant) -> bool {
        let pending: Vec<(usize, PaneId, TerminalId)> = self
            .state
            .workspaces
            .iter()
            .enumerate()
            .flat_map(|(ws_idx, ws)| {
                ws.public_pane_numbers
                    .keys()
                    .copied()
                    .filter_map(move |pane_id| {
                        let terminal_id = ws.terminal_id(pane_id)?;
                        Some((ws_idx, pane_id, terminal_id.clone()))
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|(_, _, terminal_id)| {
                self.state
                    .terminals
                    .get(terminal_id)
                    .is_some_and(|terminal| terminal.send_queue_depth() > 0)
            })
            .collect();

        let mut delivered_any = false;
        let mut remaining_any = false;
        for (ws_idx, pane_id, terminal_id) in pending {
            let outcome = self.flush_terminal_send_queue(ws_idx, pane_id, terminal_id, now);
            delivered_any |= outcome.delivered > 0;
            remaining_any |= outcome.remaining > 0;
        }

        self.next_send_queue_flush = remaining_any.then(|| now + SEND_QUEUE_FLUSH_INTERVAL);
        delivered_any
    }

    pub(crate) fn schedule_send_queue_flush(&mut self, now: Instant) {
        let deadline = now + SEND_QUEUE_FLUSH_INTERVAL;
        if self
            .next_send_queue_flush
            .is_none_or(|current| current > deadline)
        {
            self.next_send_queue_flush = Some(deadline);
        }
    }

    /// Flush one terminal's queue if its gate is open. The gate is evaluated
    /// once per flush so that our own writes (which fill the composer until
    /// a queued Enter submits them) cannot re-close the gate mid-drain.
    fn flush_terminal_send_queue(
        &mut self,
        ws_idx: usize,
        pane_id: PaneId,
        terminal_id: TerminalId,
        now: Instant,
    ) -> FlushOutcome {
        let gate_open = {
            let Some(terminal) = self.state.terminals.get(&terminal_id) else {
                return FlushOutcome::default();
            };
            if terminal.send_queue_depth() == 0 {
                return FlushOutcome::default();
            }
            let Some(runtime) =
                self.state
                    .runtime_for_pane_in_workspace(&self.terminal_runtimes, ws_idx, pane_id)
            else {
                return FlushOutcome {
                    delivered: 0,
                    remaining: terminal.send_queue_depth(),
                };
            };
            let composer =
                detect_composer_state(terminal.effective_known_agent(), &runtime.detection_text());
            terminal.send_gate_open(composer, now)
        };
        if !gate_open {
            let remaining = self
                .state
                .terminals
                .get(&terminal_id)
                .map(|terminal| terminal.send_queue_depth())
                .unwrap_or(0);
            return FlushOutcome {
                delivered: 0,
                remaining,
            };
        }

        let mut delivered = 0;
        while let Some(send) = self
            .state
            .terminals
            .get_mut(&terminal_id)
            .and_then(|terminal| terminal.pop_pending_send())
        {
            match self.write_queued_send(ws_idx, pane_id, &send) {
                Ok(()) => {
                    if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
                        terminal.note_agent_send(now);
                    }
                    delivered += 1;
                }
                Err(unwritten) => {
                    if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
                        if let Some(unwritten) = unwritten {
                            terminal.requeue_pending_send_front(unwritten);
                        }
                        if delivered > 0 {
                            terminal.note_agent_send(now);
                        }
                    }
                    break;
                }
            }
        }

        let remaining = self
            .state
            .terminals
            .get(&terminal_id)
            .map(|terminal| terminal.send_queue_depth())
            .unwrap_or(0);
        if delivered > 0 {
            self.emit_send_queue_event(
                ws_idx,
                pane_id,
                crate::api::schema::EventKind::PaneSendFlushed,
                delivered,
                remaining,
            );
        }
        FlushOutcome {
            delivered,
            remaining,
        }
    }

    /// Write one queued send to the pane's PTY, encoding text and keys with
    /// the terminal's current input modes. On a failed write, returns the
    /// unwritten portion to requeue (`None` when nothing remains that can be
    /// safely retried without duplicating already-written bytes).
    fn write_queued_send(
        &self,
        ws_idx: usize,
        pane_id: PaneId,
        send: &QueuedSend,
    ) -> Result<(), Option<QueuedSend>> {
        let Some(runtime) =
            self.state
                .runtime_for_pane_in_workspace(&self.terminal_runtimes, ws_idx, pane_id)
        else {
            return Err(Some(send.clone()));
        };
        if !send.text.is_empty() {
            let text_bytes = if send.encode_text {
                encode_api_text(runtime, &send.text)
            } else {
                send.text.clone().into_bytes()
            };
            if runtime.try_send_bytes(Bytes::from(text_bytes)).is_err() {
                return Err(Some(send.clone()));
            }
        }
        let encoded_keys = match encode_api_keys(runtime, &send.keys) {
            Ok(encoded_keys) => encoded_keys,
            Err(key) => {
                // Keys were validated at enqueue; a failure here means the
                // key table changed underneath us. Drop the keys, keep going.
                warn!(key, "queued send key no longer encodable; dropping keys");
                Vec::new()
            }
        };
        for (index, bytes) in encoded_keys.into_iter().enumerate() {
            if runtime.try_send_bytes(Bytes::from(bytes)).is_err() {
                return Err(Some(QueuedSend {
                    text: String::new(),
                    encode_text: false,
                    keys: send.keys[index..].to_vec(),
                    enqueued_at: send.enqueued_at,
                }));
            }
        }
        Ok(())
    }

    fn emit_send_queue_event(
        &mut self,
        ws_idx: usize,
        pane_id: PaneId,
        kind: crate::api::schema::EventKind,
        delivered: usize,
        queue_depth: usize,
    ) {
        let Some(public_pane_id) = self.public_pane_id(ws_idx, pane_id) else {
            return;
        };
        let workspace_id = self.public_workspace_id(ws_idx);
        let data = match kind {
            crate::api::schema::EventKind::PaneSendQueued => {
                crate::api::schema::EventData::PaneSendQueued {
                    pane_id: public_pane_id,
                    workspace_id,
                    queue_depth: queue_depth as u32,
                }
            }
            crate::api::schema::EventKind::PaneSendFlushed => {
                crate::api::schema::EventData::PaneSendFlushed {
                    pane_id: public_pane_id,
                    workspace_id,
                    delivered: delivered as u32,
                    queue_depth: queue_depth as u32,
                }
            }
            _ => return,
        };
        self.emit_event(crate::api::schema::EventEnvelope { event: kind, data });
    }
}
