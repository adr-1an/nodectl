use serde::Deserialize;

// Using signed integers for Postgres compatibility.
// Resources cannot be negative.
#[derive(Deserialize, Clone, sqlx::FromRow)]
pub struct Resources {
    pub cpu: i64, // In millicores
    pub ram: i64, // In MiB
    pub swap: i64, // In MiB
    pub disk: i64, // In MiB
}