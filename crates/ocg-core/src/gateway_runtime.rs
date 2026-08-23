//! Neutral listener-handle and rebind-port types.
//!
//! [`GatewayHandle`] is the process slot stored on `CoreState`. The async
//! [`GatewayRebindHost`] contract is implemented only by the host composition
//! adapter so `state` does not import `gateway`.

use std::future::Future;
use std::net::SocketAddr;

pub struct GatewayHandle {
    pub port: u16,
    /// Bound listen address. HTTP settings port changes rebind this IP to the
    /// configured port through [`GatewayRebindHost`].
    pub listen_addr: SocketAddr,
    /// Whether this listener is bound only to a loopback interface. The
    /// lifecycle uses this metadata to keep the shared dashboard trust mode
    /// fail-closed while replacing listeners.
    pub dashboard_is_local: bool,
    pub shutdown: tokio::sync::oneshot::Sender<()>,
    pub task: tokio::task::JoinHandle<()>,
}

/// Listener-only rebind port. The host composition adapter implements this
/// for `CoreState`; `state` calls it without naming `gateway`.
pub trait GatewayRebindHost: Sync {
    fn rebind(&self, addr: SocketAddr) -> impl Future<Output = anyhow::Result<u16>> + Send;

    fn rebind_from_serving_request(
        &self,
        addr: SocketAddr,
    ) -> impl Future<Output = anyhow::Result<u16>> + Send;
}
