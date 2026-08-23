//! Test-only listener for direct legacy Dashboard handler coverage.
//!
//! Production Gateway listeners always compose through `host_router`, so this
//! helper cannot bypass the Dashboard V2 retirement boundary outside tests.

#![allow(dead_code)]

use ocg_core::state::CoreStateInner;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub struct LegacyDashboardHandle {
    pub port: u16,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl LegacyDashboardHandle {
    pub async fn start(state: Arc<CoreStateInner>) -> Self {
        state.set_dashboard_local_mode(true);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("legacy dashboard test listener should bind");
        let port = listener
            .local_addr()
            .expect("legacy dashboard test listener should have an address")
            .port();
        let app = axum::Router::new()
            .nest(
                "/dashboard/api",
                ocg_core::dashboard::api_router(state.clone()),
            )
            .with_state(state);
        let (shutdown, shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("legacy dashboard test listener should serve");
        });
        Self {
            port,
            shutdown,
            task,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}/dashboard/api{path}", self.port)
    }

    pub async fn stop(self) {
        let _ = self.shutdown.send(());
        self.task
            .await
            .expect("legacy dashboard test listener should stop");
    }
}
