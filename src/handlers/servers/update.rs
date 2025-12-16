use axum::extract::{State, Path};
use axum::http::StatusCode;
use axum::Json;
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::json;
use tokio::process::Command;
use crate::AppState;
use crate::helpers::servers;
use crate::logs::logs;
use crate::structs::network::{Allocation, NetworkMode};
use crate::structs::resources::Resources;
use crate::structs::server::RestartPolicy;
use crate::structs::server_service::ServerService;

#[derive(Deserialize)]
pub struct UpdateServerPayload {
    restart_policy: RestartPolicy,

    resources: Option<Resources>,

    network_mode: NetworkMode,
    add_allocations: Option<Vec<Allocation>>,
    remove_allocations: Option<Vec<Allocation>>,
}

pub async fn update_server_container_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<UpdateServerPayload>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    // State vars
    let db = &state.db;
    let cfg = &state.cfg;

    // Destructure payload
    let UpdateServerPayload {
        restart_policy,

        resources,

        network_mode,
        add_allocations,
        remove_allocations,
    } = p;

    // Build server struct
    let server_service = ServerService::new(db.to_owned());
    let mut server = match server_service.get_server(&id).await {
        Ok(srv) => srv,
        Err(e) => {
            logs::error(&format!(
                "Failed to get server [{}]: {}",
                &id, e,
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to get server.",
                }))
            ).into_response());
        }
    };

    // Validate resources
    // If they're not present in the payload,
    // copy resources from the server struct.
    let resources = match resources {
        Some(r) => {
            if r.cpu < -1 || r.ram < -1 || r.swap < -1 || r.disk < -1 {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({
                    "error": "Resources cannot be lower than -1.",
                }))
                ).into_response());
            }
            r
        }
        None => server.resources.clone(),
    };

    // Begin tx
    let mut tx = match db.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            logs::error(&format!(
                "Failed to start DB transaction: {}",
                e,
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to start database transaction.",
                }))
            ).into_response());
        }
    };

    // --- Database ---

    // 1. Update server
    if let Err(e) = sqlx::query(r#"
        UPDATE servers
        SET restart_policy = $1, network_mode = $2
        WHERE id = $3
    "#)
        .bind(restart_policy.as_str())
        .bind(network_mode.as_str())
        .bind(&id)
        .execute(&mut *tx)
        .await {
        logs::error(&format!(
            "Failed to update server [{}]: {}",
            &id, e,
        ));
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
            "error": "Failed to update server.",
        }))
        ).into_response());
    }

    // 2. Add allocations, if any provided
    for alloc in add_allocations.iter().flatten() {
        if let Err(e) = sqlx::query(r#"
            INSERT INTO allocations (server_id, port, protocol)
            VALUES ($1, $2, $3)
            ON CONFLICT DO NOTHING
        "#)
            .bind(&id)
            .bind(alloc.port)
            .bind(alloc.protocol.as_str())
            .execute(&mut *tx)
            .await {
            logs::error(&format!(
                "Failed to insert allocation [{}/{}] for server [{}]: {}",
                alloc.port, alloc.protocol.as_str(), id, e,
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                "error": "Failed to store allocation.",
            }))
            ).into_response());
        }
    }

    // Remove allocations, if any provided
    for alloc in remove_allocations.iter().flatten() {
        if let Err(e) = sqlx::query(r#"
            DELETE FROM allocations
            WHERE server_id = $1
            AND port = $2
            AND protocol = $3
        "#)
            .bind(&id)
            .bind(alloc.port)
            .bind(alloc.protocol.as_str())
            .execute(&mut *tx)
            .await {
            logs::error(&format!(
                "Failed to delete allocation [{}/{}] for server [{}]: {}",
                alloc.port, alloc.protocol.as_str(), id, e,
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                "error": "Failed to delete allocation.",
            }))
            ).into_response());
        }
    }

    // --- Docker ---

    // If the container ID is missing for whatever reason, resort to container name.
    if server.container_id.is_none() {
        let name = format!("nodectl-server-{}", id);
        server.container_id = Some(name.to_string());
    }

    // Delete old container
    match Command::new("docker")
        .args([
            "rm",
            "-f",
            &server.container_id.unwrap_or_default()
        ])
        .output()
        .await {
        Ok(o) if o.status.success() => {
            logs::verbose(cfg.verbose, &format!(
                "Temporarily deleted container for server [{}].",
                id,
            ));
        }
        Ok(o) => {
            logs::error(&format!(
                "Failed to delete container for server [{}]: {}",
                id,
                String::from_utf8_lossy(&o.stdout)
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                        "error": "Failed to delete container.",
                    }))
            ).into_response());
        }
        Err(e) => {
            logs::error(&format!(
                "Failed to execute Docker container deletion for server [{}]: {}",
                id, e,
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                        "error": "Failed to delete container.",
                    }))
            ).into_response());
        }
    }

    // Build args

    let server_name = format!("server-{}", id);
    let data_dir = cfg.paths.servers.join(server_name).join("data");

    let args = servers::build_docker_run_args(
        cfg,
        &resources,
        &id,
        &server.blueprint,
        &data_dir,
        &restart_policy,
        &network_mode,
        &add_allocations,
        false,
    );

    // Run Docker
    let out = match Command::new("docker")
        .args(args)
        .output()
        .await {
        Ok(o) if o.status.success() => {
            logs::verbose(cfg.verbose, &format!(
                "Recreated container for server [{}].",
                &id,
            ));
            String::from_utf8_lossy(&o.stdout).to_string()
        }
        Ok(o) => {
            logs::error(&format!(
                "Failed to recreate container for server [{}]: {}",
                &id,
                String::from_utf8_lossy(&o.stderr)
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to recreate container.",
                }))
            ).into_response());
        }
        Err(e) => {
            logs::error(&format!(
                "Failed to execute container recreation for server [{}]: {}",
                &id, e,
            ));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to recreate container.",
                }))
            ).into_response());
        }
    };

    // Set new container ID
    if let Err(e) = sqlx::query(r#"
        UPDATE servers
        SET container_id = $1
        WHERE id = $2
    "#)
        .bind(&out.trim())
        .bind(&id)
        .execute(&mut *tx)
        .await {
        logs::error(&format!(
            "Failed to update server container ID: {}",
            e,
        ));
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "Failed to update container ID.",
            }))
        ).into_response());
    }

    // Commit tx
    if let Err(e) = tx.commit().await {
        logs::error(&format!(
            "Failed to commit DB transaction: {}",
            e,
        ));
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "Failed to commit database transaction.",
            }))
        ).into_response());
    }

    logs::verbose(cfg.verbose, &format!(
        "Updated server [{}].",
        &id,
    ));

    Ok(StatusCode::NO_CONTENT)
}