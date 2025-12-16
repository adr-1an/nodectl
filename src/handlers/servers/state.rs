use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use axum::response::IntoResponse;
use tokio::process::Command;
use serde::Deserialize;
use serde_json::json;
use crate::AppState;
use crate::logs::logs;
use crate::structs::server::ServerState;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StateOps {
    Start,
    Stop,
    Restart,
    Terminate,
}

impl StateOps {
    fn to_state(&self) -> &'static str {
        match self {
            Self::Start => ServerState::Starting.as_str(),
            Self::Stop => ServerState::Stopping.as_str(),
            Self::Restart => ServerState::Restarting.as_str(),
            Self::Terminate => ServerState::Stopped.as_str(),
        }
    }
}

#[derive(Deserialize)]
pub struct StatePayload {
    pub operation: StateOps,
    pub use_stop_command: Option<bool>,
}

async fn docker_cmd(args: &[&str]) -> Result<(), String> {
    let out = Command::new("docker").args(args).output().await
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        return Ok(());
    }

    Err(String::from_utf8_lossy(&out.stderr).to_string())
}

pub fn is_running(state: &str) -> bool {
    matches!(state, "running" | "restarting" | "paused")
}

pub fn is_stopped(state: &str) -> bool {
    matches!(state, "exited" | "dead")
}

async fn wait_for_exit(container_id: &str) -> Result<(), String> {
    loop {
        let out = Command::new("docker")
            .args(["inspect", "-f", "{{.State.Status}}", container_id])
            .output()
            .await
            .map_err(|e| e.to_string())?;

        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).to_string());
        }

        let state = String::from_utf8_lossy(&out.stdout)
            .trim()
            .to_string();

        if state == "exited" || state == "dead" {
            return Ok(());
        }

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
}

pub async fn server_state_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<StatePayload>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    // State vars
    let db = &state.db;
    let cfg = &state.cfg;

    // Get the server's container ID and stop command
    let (container_id, stop_command): (String, Option<String>) = match sqlx::query_as(r#"
        SELECT s.container_id, b.stop_command
        FROM servers s
        JOIN blueprints b ON b.server_id = s.id
        WHERE s.id = $1
    "#)
    .bind(&id)
    .fetch_one(db)
    .await {
        Ok(res) => res,
        Err(sqlx::Error::RowNotFound) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "Server not found."
                }))
            ).into_response())
        }
        Err(e) => {
            logs::error(&format!(
                "Failed to fetch container ID: {}", e,
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to fetch the server's container ID.",
                }))
            ).into_response())
        }
    };

    // Get the container's current state
    let container_state = match Command::new("docker")
    .args([
        "inspect",
        "-f",
        "{{.State.Status}}",
        &container_id,
    ])
    .output()
    .await {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        Ok(o) => {
            logs::error(&format!(
                "Failed to get container [{}] state: {}",
                &container_id,
                String::from_utf8_lossy(&o.stderr)
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to get container state.",
                }))
            ).into_response())
        }
        Err(e) => {
            logs::error(&format!{
                "Failed to execute container [{}] state check: {}",
                &container_id, e,
            });
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to get container state.",
                }))
            ).into_response())
        }
    };

    // Perform the action on the Docker container
    match p.operation {
        StateOps::Start => {
            if !is_running(&container_state) {
                if let Err(e) = docker_cmd(&["start", &container_id]).await {
                    logs::error(&format!(
                        "Failed to start container [{}]: {}",
                        &container_id, e,
                    ));
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "error": "Failed to start container.",
                        }))
                    ).into_response())
                }
                logs::verbose(cfg.verbose, &format!("Started container [{}].", &container_id));
            }
        }

        StateOps::Stop => {
            if !is_stopped(&container_state) {
                let mut stopped_via_stdin = false;

                if p.use_stop_command.unwrap_or(false) {
                    if let Some(cmd) = stop_command.as_deref() {
                        if let Err(e) = docker_cmd(&[
                            "exec",
                            "-i",
                            "-t",
                            &container_id,
                            "sh",
                            "-c",
                            &format!("echo '{}' ", cmd.replace("'", "'\\''")),
                        ])
                            .await
                        {
                            logs::error(&format!(
                                "Failed to send stdin stop command to container [{}]: {}",
                                &container_id, e,
                            ));
                        } else {
                            stopped_via_stdin = true;
                            logs::verbose(
                                cfg.verbose,
                                &format!(
                                    "Sent stdin stop command to container [{}].",
                                    &container_id
                                ),
                            );
                        }
                    }
                }

                if !stopped_via_stdin {
                    if let Err(e) =
                        docker_cmd(&["kill", "--signal=SIGTERM", &container_id]).await
                    {
                        logs::error(&format!(
                            "Failed to send SIGTERM to container [{}]: {}",
                            &container_id, e,
                        ));
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({
                        "error": "Failed to stop container.",
                    }))
                        ).into_response());
                    }

                    logs::verbose(
                        cfg.verbose,
                        &format!("Sent SIGTERM to container [{}].", &container_id),
                    );
                }
            }
        }

        StateOps::Restart => {
            if !is_stopped(&container_state) {
                docker_cmd(&["kill", "--signal=SIGTERM", &container_id])
                    .await
                    .map_err(|e| {
                        logs::error(&format!("Failed to SIGTERM [{}]: {}", container_id, e));
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": "Failed to stop container." }))
                        ).into_response()
                    })?;

                if let Err(e) = wait_for_exit(&container_id).await {
                    logs::error(&format!("Wait-for-exit failed [{}]: {}", container_id, e));
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "Failed while waiting for container to stop." }))
                    ).into_response());
                }
            }

            docker_cmd(&["start", &container_id])
                .await
                .map_err(|e| {
                    logs::error(&format!("Failed to start [{}]: {}", container_id, e));
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "Failed to start container." }))
                    ).into_response()
                })?;
            logs::verbose(cfg.verbose, &format!("Restarted container [{}].", &container_id));
        }

        StateOps::Terminate => {
            if !is_stopped(&container_state) {
                if let Err(e) = docker_cmd(&["kill", "--signal=SIGKILL", &container_id]).await {
                    logs::error(&format!(
                        "Failed to send SIGKILL to container [{}]: {}",
                        &container_id, e,
                    ));
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "error": "Failed to terminate container.",
                        }))
                    ).into_response());
                }
                logs::verbose(cfg.verbose, &format!("Terminated container [{}].", &container_id));
            }
        }
    }

    // Update server state in DB
    if let Err(e) = sqlx::query(r#"
        UPDATE servers
        SET state = $1
        WHERE id = $2
    "#)
        .bind(p.operation.to_state())
        .bind(&id)
        .execute(db)
        .await {
        logs::error(&format!(
            "Failed to update server [{}] state: {}",
            id, e,
        ));
    }
    logs::verbose(cfg.verbose, &format!("Updated container [{}] DB state.", &container_id));

    Ok(StatusCode::NO_CONTENT.into_response())
}