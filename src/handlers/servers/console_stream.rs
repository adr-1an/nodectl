use axum::extract::{Path, State, WebSocketUpgrade};
use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use axum::response::IntoResponse;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::AppState;
use crate::logs::logs;

async fn is_container_running(name: &str) -> bool {
    match Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", name])
        .output()
        .await
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == "true",
        Err(_) => false,
    }
}

async fn handle_console(mut socket: WebSocket, state: AppState, server_id: String) {
    let _cfg = &state.cfg;
    let container_name = format!("nodectl-server-{}", server_id);

    let mut poll = interval(Duration::from_secs(2));
    let mut was_running = false;
    let mut sent_stopped_once = false;

    loop {
        tokio::select! {
            msg = socket.recv() => {
                // if client disconnects, stop everything
                if msg.is_none() {
                    return;
                }
            }

            _ = poll.tick() => {
                let running = is_container_running(&container_name).await;

                if running && !was_running {
                    sent_stopped_once = false;
                    let _ = socket
                        .send(Message::Text(Utf8Bytes::from("[NodeCTL] Container started.\n")))
                        .await;

                    let mut child = match Command::new("docker")
                        .args(["attach", &container_name])
                        .stdin(std::process::Stdio::piped())
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .spawn()
                    {
                        Ok(c) => {
                            was_running = true;
                            c
                        }
                        Err(e) => {
                            logs::warn(&format!("Failed to attach to {}: {}", container_name, e));
                            was_running = false;
                            continue;
                        }
                    };

                    let mut stdin = match child.stdin.take() {
                        Some(s) => s,
                        None => {
                            let _ = child.kill().await;
                            was_running = false;
                            continue;
                        }
                    };

                    let mut stdout = match child.stdout.take() {
                        Some(s) => s,
                        None => {
                            let _ = child.kill().await;
                            was_running = false;
                            continue;
                        }
                    };

                    let mut stderr = match child.stderr.take() {
                        Some(s) => s,
                        None => {
                            let _ = child.kill().await;
                            was_running = false;
                            continue;
                        }
                    };

                    // pipe stdout/stderr -> single stream -> websocket
                    let (out_tx, mut out_rx) = mpsc::channel::<String>(256);

                    {
                        let out_tx = out_tx.clone();
                        tokio::spawn(async move {
                            let mut buf = [0u8; 4096];
                            loop {
                                match stdout.read(&mut buf).await {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        let s = String::from_utf8_lossy(&buf[..n]).to_string();
                                        if out_tx.send(s).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        });
                    }

                    {
                        let out_tx = out_tx.clone();
                        tokio::spawn(async move {
                            let mut buf = [0u8; 4096];
                            loop {
                                match stderr.read(&mut buf).await {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        let s = String::from_utf8_lossy(&buf[..n]).to_string();
                                        if out_tx.send(s).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        });
                    }

                    // console loop while attached
                    loop {
                        tokio::select! {
                            // send output chunks to websocket
                            out = out_rx.recv() => {
                                match out {
                                    Some(chunk) => {
                                        if socket.send(Message::Text(Utf8Bytes::from(chunk))).await.is_err() {
                                            let _ = child.kill().await;
                                            return;
                                        }
                                    }
                                    None => {
                                        // output tasks ended
                                        break;
                                    }
                                }
                            }

                            // receive websocket commands -> stdin
                            msg = socket.recv() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        // TTY-friendly newline
                                        let mut cmd = text.to_string();
                                        while cmd.ends_with('\n') || cmd.ends_with('\r') {
                                            cmd.pop();
                                        }
                                        cmd.push_str("\r\n");
                                        let _ = stdin.write_all(cmd.as_bytes()).await;
                                        let _ = stdin.flush().await;
                                    }
                                    Some(Ok(Message::Ping(p))) => {
                                        let _ = socket.send(Message::Pong(p)).await;
                                    }
                                    Some(Ok(_)) => {}
                                    _ => {
                                        let _ = child.kill().await;
                                        return;
                                    }
                                }
                            }

                            // detect container stop
                            _ = poll.tick() => {
                                if !is_container_running(&container_name).await {
                                    break;
                                }
                            }

                            // detect attach exiting
                            status = child.wait() => {
                                let _ = status;
                                break;
                            }
                        }
                    }

                    let _ = child.kill().await;
                    
                    if is_container_running(&container_name).await {
                        was_running = false;
                    }
                }

                if !running && was_running {
                    was_running = false;
                }

                if !running && !sent_stopped_once {
                    sent_stopped_once = true;
                    let _ = socket
                        .send(Message::Text(Utf8Bytes::from("[NodeCTL] Container stopped.\n")))
                        .await;
                }
            }
        }
    }
}

pub async fn console_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let cfg = &state.cfg;
    logs::verbose(cfg.verbose, &format!("Console websocket connected for server [{}].", id));
    ws.on_upgrade(move |socket| handle_console(socket, state, id))
}
