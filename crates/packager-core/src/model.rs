use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRecipe {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub category: String,
    pub homepage: String,
    pub license: String,
    pub runtime: RuntimeRecipe,
    pub ui: UiRecipe,
    #[serde(default)]
    pub requirements: Requirements,
    #[serde(default)]
    pub updates: UpdateRecipe,
    #[serde(default)]
    pub secrets: Vec<SecretRecipe>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SecretRecipe {
    pub key: String,
    #[serde(default = "default_secret_generator")]
    pub generate: String,
}

fn default_secret_generator() -> String {
    "uuid".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRecipe {
    pub kind: String,
    pub compose_file: String,
    pub project_name: String,
    #[serde(default)]
    pub ports: Vec<PortRecipe>,
    #[serde(default)]
    pub host_services: Vec<HostServiceRecipe>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortRecipe {
    pub name: String,
    pub container_port: u16,
    pub environment: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostServiceRecipe {
    pub name: String,
    pub service: String,
    pub port: u16,
    pub environment: String,
    #[serde(default = "default_host_service_protocol")]
    pub protocol: String,
}

fn default_host_service_protocol() -> String {
    "http".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UiRecipe {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub port: Option<String>,
    #[serde(default = "default_path")]
    pub path: String,
    #[serde(default = "default_width")]
    pub width: f64,
    #[serde(default = "default_height")]
    pub height: f64,
}

fn default_path() -> String {
    "/".into()
}

fn default_width() -> f64 {
    1280.0
}

fn default_height() -> f64 {
    820.0
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Requirements {
    #[serde(default)]
    pub memory_mb: u64,
    #[serde(default)]
    pub disk_mb: u64,
    #[serde(default)]
    pub architectures: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateRecipe {
    #[serde(default = "default_update_strategy")]
    pub strategy: String,
    #[serde(default = "default_update_interval")]
    pub interval_hours: u64,
}

impl Default for UpdateRecipe {
    fn default() -> Self {
        Self {
            strategy: default_update_strategy(),
            interval_hours: default_update_interval(),
        }
    }
}

fn default_update_strategy() -> String {
    "compose-pull".into()
}

fn default_update_interval() -> u64 {
    24
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InstalledState {
    pub installed_version: String,
    pub installed_at: u64,
    pub automatic_updates: bool,
    pub last_update_check: Option<u64>,
    #[serde(default)]
    pub environment: HashMap<String, String>,
    #[serde(default)]
    pub ports: HashMap<String, u16>,
    #[serde(default)]
    pub secret_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub category: String,
    pub homepage: String,
    pub license: String,
    pub memory_mb: u64,
    pub disk_mb: u64,
    pub installed: bool,
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub category: String,
    pub status: String,
    pub automatic_updates: bool,
    pub url: String,
    pub last_update_check: Option<u64>,
    pub icon_data_url: Option<String>,
    pub original_icon_data_url: Option<String>,
    pub custom_icon: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub engine_available: bool,
    pub engine_name: String,
    pub engine_version: Option<String>,
    pub app_data_dir: String,
    pub runtime: ManagedRuntimeStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimeStatus {
    pub installed: bool,
    pub running: bool,
    pub state: String,
    pub version: Option<String>,
    pub details: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAnalysis {
    pub name: String,
    pub image: Option<String>,
    pub ports: Vec<u16>,
    pub volumes: Vec<String>,
    pub environment: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderAnalysis {
    pub source: String,
    pub detected_name: String,
    pub services: Vec<ServiceAnalysis>,
    pub candidate_ports: Vec<u16>,
    pub warnings: Vec<String>,
    pub detected_icon: Option<String>,
    pub icon_preview_data_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderRequest {
    pub source_kind: String,
    pub source: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub homepage: String,
    pub container_port: u16,
    #[serde(default)]
    pub secret_keys: Vec<String>,
    #[serde(default)]
    pub icon_data: Option<String>,
}
