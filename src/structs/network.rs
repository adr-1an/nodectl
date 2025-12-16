use serde::Deserialize;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::Tcp => "tcp",
            Protocol::Udp => "udp",
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    // No network config at all. No traffic in/out.
    // No allocation.
    None,

    // Only outgoing traffic.
    // No allocation.
    Private,

    // Both incoming & outgoing.
    // At least 1 allocation required.
    Public,
}

impl NetworkMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            NetworkMode::None => "none",
            NetworkMode::Private => "private",
            NetworkMode::Public => "public",
        }
    }
}

#[derive(Deserialize, Clone, sqlx::FromRow)]
pub struct Allocation {
    pub port: i32,
    pub protocol: Protocol,
}