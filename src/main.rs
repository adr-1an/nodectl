use std::process::Command;
use dashmap::DashMap;
use nix::unistd::Uid;
use crate::config::{load_config, SharedConfig};
use crate::db::init_db;
use crate::internal::constants::{ASCII_LOGO, VERSION};
use crate::logs::logs::{error, info, verbose};
use crate::routes::router;

mod internal;
mod config;
mod logs;
mod routes;
mod mw;
mod handlers;
mod db;
mod structs;
mod helpers;

#[derive(Clone)]
pub struct AppState {
    cfg: SharedConfig,
    db: sqlx::PgPool,
    console_txs: DashMap<String, tokio::sync::mpsc::Sender<String>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", ASCII_LOGO.replace("{ver}", VERSION));

    // Check root privileges
    if !Uid::effective().is_root() {
        error(
            "NodeCTL requires root privileges.\n\
            Please re-run it as root."
        );
        std::process::exit(1);
    }
    
    // Load config
    let cfg = load_config()?;
    verbose(cfg.verbose, "Config loaded.");

    // Init DB
    verbose(cfg.verbose, "Connecting to database...");
    let db = init_db(&cfg).await?;
    verbose(cfg.verbose, "Database connected.");

    // Test Docker command
    let out = Command::new("docker")
        .arg("version")
        .output()
        .expect("Failed to execute docker.");

    if !out.status.success() {
        eprintln!("Fatal: Docker daemon is not running.");
        std::process::exit(1);
    }
    
    // Create state
    let state = AppState {
        cfg: cfg.clone(),
        db,
        console_txs: DashMap::new(),
    };

    // Init router
    let app = router(state);

    // Start server
    let addr = format!("0.0.0.0:{}", cfg.app_port);
    info(&format!("Starting server on {}.", addr));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app).await?;

    Ok(())
}