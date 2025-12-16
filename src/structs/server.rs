use serde::Deserialize;
use crate::structs::{
    blueprint,
    network,
    resources,
};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    No,
    OnFailure,
    Always,
    UnlessStopped,
}

impl RestartPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            RestartPolicy::No => "no",
            RestartPolicy::OnFailure => "on-failure",
            RestartPolicy::Always => "always",
            RestartPolicy::UnlessStopped => "unless-stopped",
        }
    }
}

#[derive(Deserialize, Clone)]
pub enum InstallStatus {
    Pending,
    Running,
    Finished,
    Failed,
}

impl InstallStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            InstallStatus::Pending => "pending",
            InstallStatus::Running => "running",
            InstallStatus::Finished => "finished",
            InstallStatus::Failed => "failed",
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ServerState {
    Installing,
    Running,
    Starting,
    Stopping,
    Restarting,
    Stopped,
    Restoring,
}

impl ServerState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ServerState::Installing => "installing",
            ServerState::Running => "running",
            ServerState::Starting => "starting",
            ServerState::Stopping => "stopping",
            ServerState::Restarting => "restarting",
            ServerState::Stopped => "stopped",
            ServerState::Restoring => "restoring",
        }
    }
}

#[derive(Clone)]
pub struct Server {
    pub id: String,
    pub container_id: Option<String>,

    pub state: ServerState,
    pub install_status: InstallStatus,
    pub restart_policy: RestartPolicy,

    pub blueprint: blueprint::Blueprint,

    pub network_mode: network::NetworkMode,
    pub allocations: Option<Vec<network::Allocation>>,

    pub resources: resources::Resources, // CPU, RAM, Disk
}
