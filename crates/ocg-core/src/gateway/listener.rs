//! Listener-only Gateway lifecycle.
//!
//! [`GatewayLifecycle::bind`] owns TCP bind, dashboard local-mode, forward-log
//! backfill, and the HTTP server task. It does not start or cancel process-level
//! workers. [`GatewayLifecycle::stop`] is signal-only.

use crate::state::{CoreState, GatewayHandle};
use anyhow::Result;
use std::net::SocketAddr;
use tokio::sync::oneshot;

pub struct GatewayLifecycle;

impl GatewayLifecycle {
    pub async fn bind(state: CoreState, addr: SocketAddr) -> Result<GatewayHandle> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;
        state.set_dashboard_local_mode(local_addr.ip().is_loopback());
        spawn_forward_log_backfill(state.clone());
        let app = super::build_router(state);
        let port = local_addr.port();

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let handle = tokio::spawn(async move {
            let server = axum::serve(listener, app);
            let server = server.with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            });
            if let Err(e) = server.await {
                eprintln!("gateway server error: {}", e);
            }
        });

        Ok(GatewayHandle {
            port,
            shutdown: shutdown_tx,
            task: handle,
        })
    }

    pub fn stop(handle: GatewayHandle) {
        let _ = handle.shutdown.send(());
        // Important: don't block_on the JoinHandle — stop_gateway is called from
        // tokio runtime contexts and ExitRequested handlers.
        // The spawned task will exit when the graceful-shutdown future resolves.
        // If blocking is needed later, spawn the wait on a dedicated std::thread.
    }
}

/// Attributes pre-multi-key forward logs to the primary key in bounded
/// chunks. The runtime context (not `CoreStateInner` construction) owns this
/// so pure synchronous construction never starts it. Small tables finish
/// inline; a dedicated thread (holding only a weak state reference) continues
/// large ones chunk by chunk so request logging never waits behind more than
/// one short transaction. DB work is synchronous, so no lock is ever held
/// across an await point.
fn spawn_forward_log_backfill(state: CoreState) {
    // The primary key attributes under its fixed hardcoded id (see
    // `gateway_keys::PRIMARY_KEY_ID`); no config or db lock is needed.
    let (key_id, key_name) = (
        crate::kernel::ids::PRIMARY_KEY_ID.to_string(),
        crate::kernel::ids::PRIMARY_KEY_NAME.to_string(),
    );
    // Fast path: one bounded step inline. Fresh databases (and tests) have no
    // NULL rows, so this records the completion marker and never spawns.
    let more_chunks = {
        let db = state.db.lock();
        db.backfill_forward_logs_client_key_step(
            &key_id,
            &key_name,
            crate::db::FORWARD_LOG_BACKFILL_CHUNK_ROWS,
        )
    };
    match more_chunks {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            eprintln!("warning: forward log key backfill unavailable: {error}");
            return;
        }
    }
    let weak = std::sync::Arc::downgrade(&state);
    std::thread::Builder::new()
        .name("ocg-forward-log-backfill".to_string())
        .spawn(move || {
            while let Some(state) = weak.upgrade() {
                let step = {
                    let db = state.db.lock();
                    db.backfill_forward_logs_client_key_step(
                        &key_id,
                        &key_name,
                        crate::db::FORWARD_LOG_BACKFILL_CHUNK_ROWS,
                    )
                };
                match step {
                    Ok(true) => std::thread::sleep(crate::db::FORWARD_LOG_BACKFILL_CHUNK_PAUSE),
                    Ok(false) => return,
                    Err(error) => {
                        eprintln!("warning: forward log key backfill paused: {error}");
                        return;
                    }
                }
            }
        })
        .ok();
}
