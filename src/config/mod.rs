use std::{env, fs};
use serde::Deserialize;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use crate::logs::logs::{error, info};

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct AuthConfig {
    pub token: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct DBConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Paths {
    pub servers: PathBuf,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Network {
    pub docker_network_mode: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub app_port: String,
    pub verbose: bool,

    pub auth: AuthConfig,

    pub database: DBConfig,

    pub paths: Paths,

    pub network: Network,

    #[serde(skip)]
    app_path: PathBuf,
}

fn validate_config(cfg: &mut Config) -> Result<(), Box<dyn Error>> {
    // Port
    if cfg.app_port.is_empty() {
        cfg.app_port = String::from("9924");
    }

    // Auth
    if cfg.auth.token.is_empty() {
        return Err("Missing auth.token in config.toml.".into())
    }
    if cfg.auth.token.len() < 8 {
        return Err("Config field auth.token must be at least 8 characters long.".into());
    }

    // DB
    if cfg.database.host.is_empty() {
        cfg.database.host = String::from("127.0.0.1");
    }
    if cfg.database.port == 0 {
        cfg.database.port = 5432;
    }
    if cfg.database.user.is_empty() {
        return Err("Config field database.user is required.".into());
    }
    if cfg.database.database.is_empty() {
        cfg.database.database = String::from("nodectl");
    }

    // Paths
    if cfg.paths.servers.as_os_str().is_empty() {
        cfg.paths.servers = cfg.app_path.join("servers");
    }
    fs::create_dir_all(&cfg.paths.servers)?;

    // Network
    if !cfg.network.docker_network_mode.is_empty() {
        match cfg.network.docker_network_mode.as_str() {
            "bridge" | "host" | "none" => {}
            _ => {
                // Allow custom network names
                if cfg.network.docker_network_mode.contains(" ") {
                    return Err("Invalid docker_network_mode.".into());
                }
            }
        }
    }

    Ok(())
}

pub type SharedConfig = Arc<Config>;

pub fn load_config() -> Result<SharedConfig, Box<dyn Error>> {
    let path = env::current_dir()?;

    info(&format!("Loading config from {:?}.", path.join("config.toml")));

    // Load config.toml
    let cfg_path = path.join("config.toml");
    let config = fs::read_to_string(&cfg_path)
        .map_err(|e| format!("Failed to read config file at {:?}: {}", &cfg_path.display(), e))?;
    let mut cfg: Config = toml::from_str(&config)?;

    cfg.app_path = path.clone();

    // Validate & autofill defaults
    if let Err(e) =  validate_config(&mut cfg) {
        error(&e.to_string());
        return Err("Config validation failed.".into());
    }

    Ok(Arc::new(cfg))
}