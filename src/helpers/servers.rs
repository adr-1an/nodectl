use std::path::Path;
use tokio::process::Command;
use crate::AppState;
use crate::config::Config;
use crate::logs::logs;
use crate::structs::blueprint::Blueprint;
use crate::structs::network::{Allocation, NetworkMode};
use crate::structs::resources::Resources;
use crate::structs::server::{InstallStatus, RestartPolicy, Server, ServerState};

pub fn build_docker_run_args(
    cfg: &Config,
    resources: &Resources,
    server_id: &str,
    server_blueprint: &Blueprint,
    data_dir: &Path,
    restart_policy: &RestartPolicy,
    network_mode: &NetworkMode,
    allocations: &Option<Vec<Allocation>>,
    start_after_creation: bool,
) -> Vec<String> {
    let cmd: String;
    if start_after_creation {
        cmd = "run".into();
    } else {
        cmd = "create".into();
    }

    let mut args = vec![
        cmd,
        "--name".into(),
        format!("nodectl-server-{}", server_id),
    ];

    if start_after_creation {
        args.push("-d".into());
    }

    // --- Network ---
    // Docker network override from config
    if !cfg.network.docker_network_mode.is_empty() {
        args.push("--network".into());
        args.push(cfg.network.docker_network_mode.clone());
    }

    match network_mode {
        NetworkMode::None => {
            args.push("--network".into());
            args.push("none".into());
        }

        NetworkMode::Private => {
            // Outbound only
            // Use Docker default network
            // No ports
        }

        NetworkMode::Public => {
            // Outbound + inbound
            if let Some(allocs) = allocations {
                if !allocs.is_empty() {
                    for a in allocs {
                        args.push("-p".into());
                        args.push(format!(
                            "{}:{}/{}",
                            a.port,
                            a.port,
                            a.protocol.as_str()
                        ));
                    }
                }
            }
        }
    }

    // --- Volume ---
    let server_dir = match data_dir.parent() {
        Some(p) => p,
        None => {
            logs::error("Invalid data_dir path, no parent");
            return Vec::new();
        }
    };
    let abs_server_dir = match server_dir.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            logs::error(&format!("Failed to canonicalize server dir: {}", e));
            return Vec::new();
        }
    };

    args.push("-v".into());
    args.push(format!(
        "{}:/home/container",
        abs_server_dir.join("data").display()
    ));

    // --- Restart policy ---
    args.push("--restart".into());
    args.push(restart_policy.as_str().into());

    // Stdin
    args.push("-i".into());

    // Working directory
    args.push("-w".into());
    args.push("/home/container".into());

    // --- Resources ---
    // If a resource is -1, it won't be specified in the args,
    // making it unlimited.

    // CPU (Millicores -> cores)
    if resources.cpu > -1 {
        let cores = resources.cpu as f64 / 1000.0;
        let cpus_arg = format!("--cpus={}", cores);
        args.push(cpus_arg);
    }

    // RAM (MiB)
    if resources.ram > -1 {
        let ram_arg = format!("--memory={}m", resources.ram);
        args.push(ram_arg);
    }

    // Swap (MiB)
    if resources.swap > -1 {
        let swap_arg = format!("--memory-swap={}m", resources.ram + resources.swap);
        args.push(swap_arg);
    }

    // Disk space done using FS quotas in the creation handler.

    // --- Image ---
    args.push(server_blueprint.image_url.clone());

    // --- Command ---
    if let Some(cmd) = &server_blueprint.start_command {
        if server_blueprint.use_shell {
            args.push("sh".into());
            args.push("-c".into());
            args.push(cmd.join(" "));
        } else {
            args.extend(cmd.clone());
        }
    }

    args
}

async fn update_install_status(
    state: &AppState,
    server_id: &str,
    status: InstallStatus,
) -> Result<(), sqlx::Error> {
    let db = &state.db;

    sqlx::query(r#"
        UPDATE servers
        SET install_status = $1
        WHERE id = $2
    "#)
        .bind(status.as_str())
        .bind(server_id)
        .execute(db)
        .await?;

    Ok(())
}

async fn update_server_state(
    state: &AppState,
    server_id: &str,
    server_state: ServerState,
) -> Result<(), sqlx::Error> {
    let db = &state.db;

    sqlx::query(r#"
        UPDATE servers
        SET state = $1
        WHERE id = $2
    "#)
        .bind(server_state.as_str())
        .bind(server_id)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn install_server(
    cfg: &Config,
    state: AppState,
    server: Server,
    data_dir: &Path,
    install_img_override: &Option<String>,
    skip_installation: bool,
    start_after_install: bool
) {
    // Update install_status to running
    if let Err(e) = update_install_status(&state, &server.id, InstallStatus::Running).await {
        logs::error(&format!(
            "Failed to update install status for server [{}]: {}",
            &server.id, e
        ));
    }

    // --- INSTALLER CONTAINER ---
    // Single-shot installer container
    if !skip_installation {
        if let Some(script) = &server.blueprint.install_script {
            logs::verbose(cfg.verbose, &format!(
                "Running installer for server [{}].",
                server.id
            ));

            let script_str = script.join("\n");
            let abs_data_dir = data_dir
                .canonicalize()
                .unwrap_or_else(|_| data_dir.to_path_buf());

            let out = Command::new("docker")
                .args([
                    "run",
                    "--rm",
                    "--name",
                    &format!("nodectl-server-{}-installer", &server.id),
                    "-v",
                    &format!("{}:/home/container", abs_data_dir.display()),
                    "-w",
                    "/home/container",
                    install_img_override.clone().unwrap_or("debian:latest".to_string()).as_str(), // Default install image is Debian
                    "sh",
                    "-lc",
                    &script_str,
                ])
                .output()
                .await;

            match out {
                Ok(o) if o.status.success() => {
                    logs::verbose(cfg.verbose, "Installer finished successfully.");
                    let _ = update_install_status(&state, &server.id, InstallStatus::Finished).await;
                }
                Ok(o) => {
                    logs::error(&format!(
                        "Install script failed for server [{}]: {}",
                        server.id,
                        String::from_utf8_lossy(&o.stderr)
                    ));
                    let _ = update_install_status(&state, &server.id, InstallStatus::Failed).await;
                    return;
                }
                Err(e) => {
                    logs::error(&format!(
                        "Installer docker run failed for server [{}]: {}",
                        server.id, e
                    ));
                    let _ = update_install_status(&state, &server.id, InstallStatus::Failed).await;
                    return;
                }
            }
        }
    } else {
        let _ = update_install_status(&state, &server.id, InstallStatus::Finished).await;
    }

    // --- START RUNTIME CONTAINER ---
    let args = build_docker_run_args(
        cfg,
        &server.resources,
        &server.id,
        &server.blueprint,
        data_dir,
        &server.restart_policy,
        &server.network_mode,
        &server.allocations,
        start_after_install,
    );

    let out = match Command::new("docker")
        .args(&args)
        .output()
        .await {
        Ok(o) => o,
        Err(e) => {
            logs::error(&format!(
                "Failed to start server container [{}]: {}",
                server.id, e
            ));
            return;
        }
    };

    if !out.status.success() {
        logs::error(&String::from_utf8_lossy(&out.stderr));
        return;
    }

    let container_id = String::from_utf8_lossy(&out.stdout).trim().to_string();

    // Store container ID
    let _ = sqlx::query(
        "UPDATE servers SET container_id = $1 WHERE id = $2"
    )
        .bind(&container_id)
        .bind(&server.id)
        .execute(&state.db)
        .await;

    // Update server state
    if start_after_install {
        let _ = update_server_state(&state, &server.id, ServerState::Running).await;
    } else {
        let _ = update_server_state(&state, &server.id, ServerState::Stopped).await;
    }

    logs::info(&format!(
        "Installed server [{}].",
        server.id
    ));
}