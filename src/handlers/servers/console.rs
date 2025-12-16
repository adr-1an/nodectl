use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::json;
use crate::AppState;
use crate::logs::logs;

#[derive(Deserialize)]
pub struct SendCommandPayload {
    command: Vec<String>,
}

pub async fn send_console_command_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<SendCommandPayload>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    // State vars
    let db = &state.db;

    let exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (SELECT 1 FROM servers WHERE id = $1)"#
    )
        .bind(&id)
        .fetch_one(db)
        .await
        .map_err(|e| {
            logs::error(&format!(
                "Failed to check server [{}] existence: {}",
                &id, e,
            ));
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to check server existence.",
                }))
            ).into_response()
        })?;
    
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "Server not found."
            }))
        ).into_response());
    }

    let tx = match state
        .console_txs
        .get(&id) {
        Some(tx) => tx,
        None => {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({ "error": "Console not connected." }))
            ).into_response());
        }
    };

    if let Err(_) = tx.send(p.command.join(" "))
        .await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to send command." }))
        ).into_response());
    }


    Ok(StatusCode::NO_CONTENT.into_response())
}