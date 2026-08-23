//! Host HTTP router composition.
//!
//! Assembles the inference router with Dashboard V2/V3 mounts. This module is
//! the HTTP composition root: it depends on `gateway`, `dashboard`, and
//! `dashboard_v3`. Those modules, and `state`, must not import this module.

use crate::gateway::listener::GatewayRouterHost;
use crate::state::CoreState;
use axum::Router;
use axum::routing::get;

pub fn build_router(state: CoreState) -> Router {
    Router::new()
        .merge(crate::gateway::inference_router(state.clone()))
        .nest(
            "/dashboard/api/v3",
            crate::dashboard_v3::api_router(state.clone()),
        )
        .nest(
            "/dashboard/api",
            crate::dashboard::api_router(state.clone()),
        )
        .route("/dashboard", get(crate::dashboard::serve_index))
        .route("/dashboard/", get(crate::dashboard::serve_index))
        .route(
            "/dashboard/assets/{*path}",
            get(crate::dashboard::serve_asset),
        )
        .with_state(state)
}

impl GatewayRouterHost for CoreState {
    /// Axum assembly used by the listener. Defined here so `gateway` does not
    /// import dashboard mounts.
    fn compose_router(state: CoreState) -> Router {
        build_router(state)
    }
}
