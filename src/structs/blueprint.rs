use serde::Deserialize;

#[derive(Deserialize, Clone, sqlx::FromRow)]
pub struct Blueprint {
    pub image_url: String, // Docker image URL
    pub install_script: Option<Vec<String>>,
    pub start_command: Option<Vec<String>>, // Command used to start the main process
    pub stop_command: Option<Vec<String>>, // Command sent to stdin to stop the main process
    pub use_shell: bool,
}
