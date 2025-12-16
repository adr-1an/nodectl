use axum::{middleware, Router};
use axum::routing::{delete, get, patch, post, put};
use crate::AppState;
use crate::mw::internal_auth;
use crate::handlers::*;

pub fn router(state: AppState) -> Router {
    let cfg = &state.cfg;

    Router::new()
        // API v1
        .nest("/v1", Router::new()
            .nest("/servers", Router::new()
                // Create server
                .route("/", post(servers::create::create_server_handler))
                .nest("/{id}", Router::new()
                    // Delete server
                    .route("/", delete(servers::delete::delete_server_handler))
                    // Update server container
                    .route("/", patch(servers::update::update_server_container_handler))
                    // Start, stop, restart, terminate
                    .route("/state", put(servers::state::server_state_handler))
                    // Log stream
                    .route("/console", get(servers::console_stream::console_ws_handler))
                    // Send command
                    .route("/console", post(servers::console::send_console_command_handler))
                    // Get status
                    .route("/status", get(servers::status::server_status_handler))
                    // Live stat stream
                    .route("/stats", get(servers::stats::server_stats_ws_handler))
                )
            )
        )
        .layer(middleware::from_fn_with_state(cfg.clone(), internal_auth))
        .with_state(state)
}