//! Host adapter for listener rebind.
//!
//! Implements [`crate::gateway_runtime::GatewayRebindHost`] so `state` can
//! replace a running listener without importing `gateway`. This module may
//! import both `state` and `gateway`. It must not add inherent
//! `GatewayLifecycle` methods.

use crate::gateway::GatewayLifecycle;
use crate::gateway_runtime::GatewayRebindHost;
use crate::state::CoreState;
use std::future::Future;
use std::net::SocketAddr;

impl GatewayRebindHost for CoreState {
    fn rebind(&self, addr: SocketAddr) -> impl Future<Output = anyhow::Result<u16>> + Send {
        GatewayLifecycle::rebind(self.clone(), addr)
    }

    fn rebind_from_serving_request(
        &self,
        addr: SocketAddr,
    ) -> impl Future<Output = anyhow::Result<u16>> + Send {
        GatewayLifecycle::rebind_from_serving_request(self.clone(), addr)
    }
}
