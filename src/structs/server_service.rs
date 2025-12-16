use std::str::FromStr;
use crate::structs::blueprint::Blueprint;
use crate::structs::network::{Allocation, NetworkMode, Protocol};
use crate::structs::resources::Resources;
use crate::structs::server::{InstallStatus, RestartPolicy, Server, ServerState};

pub struct ServerService {
    db: sqlx::PgPool,
}

#[allow(dead_code)]
#[derive(sqlx::FromRow)]
struct ServerRow {
    id: String,
    container_id: String,
    state: String,
    install_status: String,
    restart_policy: String,
    network_mode: String,
}

impl FromStr for ServerState {
    type Err = sqlx::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "installing" => Ok(ServerState::Installing),
            "running" => Ok(ServerState::Running),
            "starting" => Ok(ServerState::Starting),
            "stopping" => Ok(ServerState::Stopping),
            "restarting" => Ok(ServerState::Restarting),
            "stopped" => Ok(ServerState::Stopped),
            "restoring" => Ok(ServerState::Restoring),
            _ => Err(sqlx::Error::Decode(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid server state: {}", s),
                )
            ))),
        }
    }
}

impl FromStr for InstallStatus {
    type Err = sqlx::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(InstallStatus::Pending),
            "running" => Ok(InstallStatus::Running),
            "finished" => Ok(InstallStatus::Finished),
            "failed" => Ok(InstallStatus::Failed),
            _ => Err(sqlx::Error::Decode(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("install status: {}", s),
                )
            ))),
        }
    }
}

impl FromStr for RestartPolicy {
    type Err = sqlx::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "no" => Ok(RestartPolicy::No),
            "on-failure" => Ok(RestartPolicy::OnFailure),
            "always" => Ok(RestartPolicy::Always),
            "unless-stopped" => Ok(RestartPolicy::UnlessStopped),
            _ => Err(sqlx::Error::Decode(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid restart policy: {}", s),
                )
            ))),
        }
    }
}

impl FromStr for Protocol {
    type Err = sqlx::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tcp" => Ok(Protocol::Tcp),
            "udp" => Ok(Protocol::Udp),
            _ => Err(sqlx::Error::Decode(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid protocol: {}", s),
                )
            ))),
        }
    }
}

impl FromStr for NetworkMode {
    type Err = sqlx::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(NetworkMode::None),
            "private" => Ok(NetworkMode::Private),
            "public" => Ok(NetworkMode::Public),
            _ => Err(sqlx::Error::Decode(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid network mode: {}", s),
                )
            ))),
        }
    }
}

impl ServerService {
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }

    pub async fn get_server(&self, id: &str) -> Result<Server, sqlx::Error> {
        let db = &self.db;

        // 1. Get server
        let server_row = sqlx::query_as::<_, ServerRow>(r#"
            SELECT
                id,
                container_id,
                state::text,
                install_status::text,
                restart_policy::text,
                network_mode::text
            FROM servers
            WHERE id = $1
            "#,
        )
            .bind(id)
            .fetch_one(db)
            .await?;

        // 2. Get blueprint
        let blueprint = sqlx::query_as::<_, Blueprint>(r#"
            SELECT
                image_url,

                (
                    SELECT array_agg(elem)
                    FROM jsonb_array_elements_text(install_script::jsonb) AS elem
                ) AS install_script,

                (
                    SELECT array_agg(elem)
                    FROM jsonb_array_elements_text(start_command::jsonb) AS elem
                ) AS start_command,

                (
                    SELECT array_agg(elem)
                    FROM jsonb_array_elements_text(stop_command::jsonb) AS elem
                ) AS stop_command,

                use_shell
            FROM blueprints
            WHERE server_id = $1
        "#)
            .bind(id)
            .fetch_one(db)
            .await?;

        // 3. Get allocations
        let allocs = sqlx::query_as::<_, (i32, String)>(r#"
            SELECT port, protocol::text
            FROM allocations
            WHERE server_id = $1
        "#)
            .bind(id)
            .fetch_all(db)
            .await?;

        // Get resources
        let resources = sqlx::query_as::<_, Resources>(r#"
            SELECT
                cpu::BIGINT as cpu,
                ram::BIGINT as ram,
                swap::BIGINT as swap,
                disk::BIGINT as disk
            FROM server_resources
            WHERE server_id = $1
        "#)
            .bind(id)
            .fetch_one(db)
            .await?;

        // Convert allocation rows
        let allocations = if allocs.is_empty() {
            None
        } else {
            Some(
                allocs
                    .into_iter()
                    .map(|(port, protocol)| {
                        Ok(Allocation {
                            port,
                            protocol: protocol.parse()?,
                        })
                    })
                    .collect::<Result<Vec<_>, sqlx::Error>>()?
            )
        };

        let server = Server {
            id: server_row.id,
            container_id: Option::from(server_row.container_id),

            state: server_row.state.parse()?,
            install_status: server_row.install_status.parse()?,
            restart_policy: server_row.restart_policy.parse()?,

            blueprint,

            network_mode: server_row.network_mode.parse()?,
            allocations,

            resources,
        };

        Ok(server)
    }
}