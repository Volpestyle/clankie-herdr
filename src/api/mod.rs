pub mod client;
mod event_hub;
pub mod schema;
mod server;
mod status;
mod subscriptions;
mod wait;

pub use event_hub::EventHub;
pub(crate) use server::start_server_with_stop_control;
pub use server::{start_server_with_capabilities, ServerHandle};
pub use status::{read_runtime_status_at, RuntimeStatus};

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc;

use crate::api::schema::{Method, Request};

pub const SOCKET_PATH_ENV_VAR: &str = "HERDR_SOCKET_PATH";

pub(crate) fn request_changes_ui(request: &Request) -> bool {
    matches!(
        &request.method,
        Method::ServerReloadConfig(_)
            | Method::ServerReloadAgentManifests(_)
            | Method::NotificationShow(_)
            | Method::WorkspaceCreate(_)
            | Method::WorkspaceFocus(_)
            | Method::WorkspaceRename(_)
            | Method::WorkspaceMove(_)
            | Method::WorkspaceMoveBlock(_)
            | Method::WorkspaceReportMetadata(_)
            | Method::WorkspaceClose(_)
            | Method::WorktreeCreate(_)
            | Method::WorktreeOpen(_)
            | Method::WorktreeRemove(_)
            | Method::TabCreate(_)
            | Method::TabFocus(_)
            | Method::TabRename(_)
            | Method::TabMove(_)
            | Method::TabClose(_)
            | Method::LayoutApply(_)
            | Method::LayoutSetSplitRatio(_)
            | Method::AgentRename(_)
            | Method::AgentViewSet(_)
            | Method::AgentViewClear(_)
            | Method::AgentFocus(_)
            | Method::AgentStart(_)
            | Method::AgentPrompt(_)
            | Method::AgentSendKeys(_)
            | Method::PaneSplit(_)
            | Method::PaneSwap(_)
            | Method::PaneMove(_)
            | Method::PaneZoom(_)
            | Method::PaneFocusDirection(_)
            | Method::PaneResize(_)
            | Method::PaneFocus(_)
            | Method::PaneInputSet(_)
            | Method::PaneRename(_)
            | Method::PaneGraphicsSet(_)
            | Method::PaneGraphicsClear(_)
            | Method::PaneGraphicsStream(_)
            | Method::PaneGraphicsStreamSet(_)
            | Method::PaneGraphicsStreamDirect(_)
            | Method::PaneGraphicsStreamOpen(_)
            | Method::PaneGraphicsStreamClose(_)
            | Method::PaneReportAgent(_)
            | Method::PaneReportAgentSession(_)
            | Method::PaneReportMetadata(_)
            | Method::PaneClearAgentAuthority(_)
            | Method::PaneReleaseAgent(_)
            | Method::PaneClose(_)
            | Method::PopupClose(_)
            | Method::PluginUnlink(_)
            | Method::PluginDisable(_)
            | Method::PluginActionInvoke(_)
            | Method::PluginPaneOpen(_)
            | Method::PluginPaneFocus(_)
            | Method::PluginPaneClose(_)
    )
}

pub struct ApiRequestMessage {
    pub request: Request,
    pub respond_to: std::sync::mpsc::Sender<String>,
    pub response_write_complete: Option<std::sync::mpsc::Receiver<()>>,
    pub stream_active: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub stream_to: Option<PaneOutputStreamSender>,
}

#[derive(Debug, Clone)]
pub struct PaneOutputChunk {
    pub bytes: Bytes,
}

pub struct PaneOutputStreamSender {
    tx: std::sync::mpsc::SyncSender<PaneOutputChunk>,
    queued_bytes: Arc<AtomicUsize>,
    max_queued_bytes: usize,
}

pub struct PaneOutputStreamReceiver {
    rx: std::sync::mpsc::Receiver<PaneOutputChunk>,
    queued_bytes: Arc<AtomicUsize>,
}

#[derive(Debug)]
pub enum PaneOutputStreamTrySendError {
    Full,
    Disconnected,
}

pub fn pane_output_stream_channel(
    message_capacity: usize,
    byte_capacity: usize,
) -> (PaneOutputStreamSender, PaneOutputStreamReceiver) {
    let (tx, rx) = std::sync::mpsc::sync_channel(message_capacity);
    let queued_bytes = Arc::new(AtomicUsize::new(0));
    (
        PaneOutputStreamSender {
            tx,
            queued_bytes: Arc::clone(&queued_bytes),
            max_queued_bytes: byte_capacity,
        },
        PaneOutputStreamReceiver { rx, queued_bytes },
    )
}

impl PaneOutputStreamSender {
    pub fn try_send(&self, chunk: PaneOutputChunk) -> Result<(), PaneOutputStreamTrySendError> {
        let bytes_len = chunk.bytes.len();
        let mut queued = self.queued_bytes.load(Ordering::Acquire);
        loop {
            let Some(next) = queued.checked_add(bytes_len) else {
                return Err(PaneOutputStreamTrySendError::Full);
            };
            if next > self.max_queued_bytes {
                return Err(PaneOutputStreamTrySendError::Full);
            }
            match self.queued_bytes.compare_exchange_weak(
                queued,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(current) => queued = current,
            }
        }

        match self.tx.try_send(chunk) {
            Ok(()) => Ok(()),
            Err(std::sync::mpsc::TrySendError::Full(_chunk)) => {
                self.queued_bytes.fetch_sub(bytes_len, Ordering::AcqRel);
                Err(PaneOutputStreamTrySendError::Full)
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_chunk)) => {
                self.queued_bytes.fetch_sub(bytes_len, Ordering::AcqRel);
                Err(PaneOutputStreamTrySendError::Disconnected)
            }
        }
    }

    pub fn queued_bytes(&self) -> usize {
        self.queued_bytes.load(Ordering::Acquire)
    }

    pub fn max_queued_bytes(&self) -> usize {
        self.max_queued_bytes
    }
}

impl PaneOutputStreamReceiver {
    pub fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<PaneOutputChunk, std::sync::mpsc::RecvTimeoutError> {
        let chunk = self.rx.recv_timeout(timeout)?;
        self.queued_bytes
            .fetch_sub(chunk.bytes.len(), Ordering::AcqRel);
        Ok(chunk)
    }
}

pub type ApiRequestSender = mpsc::UnboundedSender<ApiRequestMessage>;

pub fn socket_path() -> PathBuf {
    crate::session::active_api_socket_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_output_stream_channel_enforces_byte_budget() {
        let (tx, rx) = pane_output_stream_channel(8, 4);

        tx.try_send(PaneOutputChunk {
            bytes: Bytes::from_static(b"abc"),
        })
        .expect("first chunk fits");
        assert_eq!(tx.queued_bytes(), 3);

        let err = tx
            .try_send(PaneOutputChunk {
                bytes: Bytes::from_static(b"de"),
            })
            .expect_err("second chunk exceeds byte budget");
        assert!(matches!(err, PaneOutputStreamTrySendError::Full));
        assert_eq!(tx.queued_bytes(), 3);

        let chunk = rx
            .recv_timeout(Duration::from_millis(10))
            .expect("queued chunk");
        assert_eq!(chunk.bytes.as_ref(), b"abc");
        assert_eq!(tx.queued_bytes(), 0);
    }

    #[test]
    fn pane_output_stream_channel_rolls_back_bytes_when_message_queue_is_full() {
        let (tx, rx) = pane_output_stream_channel(1, 32);

        tx.try_send(PaneOutputChunk {
            bytes: Bytes::from_static(b"abc"),
        })
        .expect("first chunk fits");
        assert_eq!(tx.queued_bytes(), 3);

        let err = tx
            .try_send(PaneOutputChunk {
                bytes: Bytes::from_static(b"de"),
            })
            .expect_err("message queue is full");
        assert!(matches!(err, PaneOutputStreamTrySendError::Full));
        assert_eq!(tx.queued_bytes(), 3);

        let chunk = rx
            .recv_timeout(Duration::from_millis(10))
            .expect("queued chunk");
        assert_eq!(chunk.bytes.as_ref(), b"abc");
        assert_eq!(tx.queued_bytes(), 0);
    }
}
