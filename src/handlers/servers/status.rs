use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::process::Command;
use crate::AppState;
use crate::logs::logs;

pub async fn server_status_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    // State vars
    let db = &state.db;

    // Get container ID
    let container_id: String = match sqlx::query_scalar(r#"
        SELECT container_id FROM servers WHERE id = $1
    "#)
        .bind(&id)
        .fetch_one(db)
        .await {
        Ok(id) => id,
        Err(sqlx::error::Error::RowNotFound) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "Server not found.",
                }))
            ).into_response());
        }
        Err(e) => {
            logs::error(&format!(
                "Failed to get server [{}] container ID: {}",
                &id, e,
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to get container ID.",
                }))
            ).into_response());
        }
    };

    // --- Get container status from Docker ---
    #[derive(Serialize, Deserialize)]
    struct StatusRes {
        status: String,
        started_at: String,
    }

    let inspect_out = Command::new("docker")
        .args([
            "inspect",
            "--format={\"status\":{{json .State.Status}},\"started_at\":{{json .State.StartedAt}}}",
            &container_id,
        ])
        .output()
        .await;
    let status: StatusRes = match inspect_out {
        Ok(o) if o.status.success() => {
            serde_json::from_slice(&o.stdout).map_err(|e| {
                logs::error(&format!("Inspect JSON parse failed for server [{}]: {}", &id, e));
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Invalid docker inspect output." })),
                )
                    .into_response()
            })?
        }
        Ok(o) => {
            logs::error(&format!(
                "Docker inspect failed for server [{}]: {}",
                &id,
                String::from_utf8_lossy(&o.stderr)
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to get server status." })),
            )
                .into_response());
        }
        Err(e) => {
            logs::error(&format!("Docker inspect exec failed for server [{}]: {}", &id, e));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to get server status." })),
            )
                .into_response());
        }
    };


    Ok((StatusCode::OK, Json(json!({
        "status": status.status,
        "started_at": status.started_at,
    }))))
}