use std::time::Duration;
use axum::{
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
};
use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, BufReader};
use serde_json::json;
use tokio::time::sleep;
use crate::{logs::logs, AppState};

async fn handle_stats_ws(
    mut socket: WebSocket,
    state: AppState,
    server_id: String,
) {
    let db = &state.db;

    let container_id: String = match sqlx::query_scalar(
        "SELECT container_id FROM servers WHERE id = $1"
    )
        .bind(&server_id)
        .fetch_one(db)
        .await
    {
        Ok(id) => id,
        Err(e) => {
            logs::error(&format!("container lookup failed: {}", e));
            return;
        }
    };

    let mut child = match Command::new("docker")
        .args([
            "stats",
            &container_id,
            "--format",
            "{{.CPUPerc}}|{{.MemUsage}}|{{.MemPerc}}|{{.NetIO}}|{{.BlockIO}}|{{.PIDs}}",
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            logs::error(&format!("docker stats spawn failed: {}", e));
            return;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() != 6 {
            continue;
        }

        let payload = json!({
            "cpu": parts[0],
            "mem_usage": parts[1],
            "mem_percent": parts[2],
            "net_io": parts[3],
            "block_io": parts[4],
            "pids": parts[5],
        });

        if let Err(_) = socket
            .send(Message::Text(Utf8Bytes::from(payload.to_string())))
            .await
        {
            break;
        }
        
        sleep(Duration::from_secs(1)).await;
    }

    let _ = child.kill().await;
}


pub async fn server_stats_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_stats_ws(socket, state, id))
}
