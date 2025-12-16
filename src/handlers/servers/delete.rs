use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use tokio::fs;
use axum::Json;
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::json;
use crate::AppState;
use crate::logs::logs;

#[derive(Deserialize)]
pub struct QueryParams {
    force: Option<bool>,
}

async fn delete_from_db(db: &sqlx::PgPool, id: &str, v: bool) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;

    sqlx::query("DELETE FROM server_resources WHERE server_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM allocations WHERE server_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM blueprints WHERE server_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM servers WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    logs::verbose(v, &format!(
        "Deleted DB rows for server [{}].", id
    ));

    Ok(())
}

pub async fn delete_server_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<QueryParams>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    // State vars
    let db = &state.db;
    let cfg = &state.cfg;

    // Check query params
    let force = query.force.unwrap_or(false);

    // 1. Forcefully delete Docker container
    let container_name = format!("nodectl-server-{}", id);
    match tokio::process::Command::new("docker")
        .args([
            "rm",
            "-f",
            &container_name,
        ])
        .output()
        .await {
        Ok(o) if o.status.success() => {
            logs::verbose(cfg.verbose, &format!(
                "Successfully deleted container [{}].", container_name,
            ));
        }
        Ok(o) => {
            logs::error(&format!(
                "Failed to delete container [{}]: {}",
                container_name,
                String::from_utf8_lossy(&o.stderr),
            ));
            if force {
                logs::verbose(cfg.verbose, "Ignoring last error, forced delete.")
            } else {
                return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
                    "error": "Failed to delete container.",
                }))).into_response());
            }
        }
        Err(e) => {
            logs::error(&format!(
                "Failed to execute container [{}] deletion: {}",
                container_name,
                e,
            ));
            if force {
                logs::verbose(cfg.verbose, "Ignoring last error, forced delete.")
            } else {
                return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
                    "error": "Failed to delete container.",
                }))).into_response());
            }
        }
    }

    // 2. Delete installer container, in case one is running
    let _ = tokio::process::Command::new("docker")
        .args([
            "rm",
            "-f",
            &format!("nodectl-server-{}-installer", id)
        ])
        .output()
        .await;

    // 3. Delete server directory
    let dir_name = format!("server-{}", id);
    let server_dir = cfg.paths.servers.join(dir_name);

    if let Err(e) = fs::remove_dir_all(&server_dir).await {
        if e.kind() == tokio::io::ErrorKind::NotFound {
            logs::verbose(cfg.verbose, &format!(
                "Server directory [{}] doesn't exist, skipping deletion.", server_dir.display(),
            ));
        } else {
            logs::error(&format!(
                "Failed to delete server directory [{}]: {}",
                server_dir.display(), e,
            ));
            if force {
                logs::verbose(cfg.verbose, "Ignoring last error, forced delete.")
            } else {
                return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
                "error": "Failed to delete server directory.",
            }))).into_response());
            }
        }
    }

    // 3. Delete all DB rows belonging to this server_id
    if let Err(e) = delete_from_db(db, &id, cfg.verbose).await {
        logs::error(&format!(
            "Failed to delete server rows from DB: {e}",
        ));
        if force {
            logs::verbose(cfg.verbose, "Ignoring last error, forced delete.")
        } else {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
                "error": "Failed to delete server rows from DB.",
            }))).into_response());
        }
    }
    
    logs::info(&format!(
        "Deleted server [{}].", id,
    ));

    Ok(StatusCode::NO_CONTENT.into_response())
}