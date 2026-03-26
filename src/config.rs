use serde::Deserialize;
use std::path::PathBuf;
use crate::db::Database;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub singbox: SingboxConfig,
    pub database: DatabaseConfig,
    pub validation: ValidationConfig,
    pub quality: QualityConfig,
    pub oauth: OAuthConfig,
    #[serde(default)]
    pub subscription: SubscriptionConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub admin_password: String,
    #[serde(default = "default_min_trust_level")]
    pub min_trust_level: i32,
    #[serde(default = "default_false")]
    pub allow_registration: bool,
    #[serde(default = "default_true")]
    pub enable_oauth: bool,
}

fn default_min_trust_level() -> i32 {
    1
}

fn default_false() -> bool {
    false
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SingboxConfig {
    pub binary_path: PathBuf,
    pub config_path: PathBuf,
    pub base_port: u16,
    #[serde(default = "default_max_proxies")]
    pub max_proxies: usize,
    #[serde(default = "default_api_port")]
    pub api_port: u16,
    pub api_secret: Option<String>,
}

fn default_max_proxies() -> usize {
    300
}

fn default_api_port() -> u16 {
    9090
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ValidationConfig {
    pub url: String,
    pub timeout_secs: u64,
    pub concurrency: usize,
    pub interval_mins: u64,
    pub error_threshold: u32,
    /// How many port slots to reserve for validation/quality-check per round.
    /// The rest stay with Valid proxies serving users. Default 30.
    #[serde(default = "default_validation_batch")]
    pub batch_size: usize,
}

fn default_validation_batch() -> usize {
    30
}

#[derive(Debug, Clone, Deserialize)]
pub struct QualityConfig {
    pub interval_mins: u64,
    pub concurrency: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubscriptionConfig {
    #[serde(default)]
    pub auto_refresh_interval_mins: u64, // 0 = disabled
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            auto_refresh_interval_mins: 0,
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string("config.toml")
            .map_err(|e| format!("Failed to read config.toml: {e}"))?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }
}

/// Seeds runtime config from config.toml into DB settings table.
/// Called at startup — config.toml always overwrites DB values.
pub fn seed_settings_to_db(db: &Database, config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = std::collections::HashMap::new();

    // Server runtime settings
    settings.insert("admin_password".to_string(), config.server.admin_password.clone());
    settings.insert("min_trust_level".to_string(), config.server.min_trust_level.to_string());
    settings.insert("allow_registration".to_string(), config.server.allow_registration.to_string());
    settings.insert("enable_oauth".to_string(), config.server.enable_oauth.to_string());

    // OAuth settings
    settings.insert("oauth_client_id".to_string(), config.oauth.client_id.clone());
    settings.insert("oauth_client_secret".to_string(), config.oauth.client_secret.clone());
    settings.insert("oauth_redirect_uri".to_string(), config.oauth.redirect_uri.clone());

    // Singbox runtime settings
    settings.insert("singbox_api_secret".to_string(), config.singbox.api_secret.clone().unwrap_or_default());

    // Validation settings
    settings.insert("validation_url".to_string(), config.validation.url.clone());
    settings.insert("validation_timeout_secs".to_string(), config.validation.timeout_secs.to_string());
    settings.insert("validation_concurrency".to_string(), config.validation.concurrency.to_string());
    settings.insert("validation_interval_mins".to_string(), config.validation.interval_mins.to_string());
    settings.insert("validation_error_threshold".to_string(), config.validation.error_threshold.to_string());
    settings.insert("validation_batch_size".to_string(), config.validation.batch_size.to_string());

    // Quality settings
    settings.insert("quality_interval_mins".to_string(), config.quality.interval_mins.to_string());
    settings.insert("quality_concurrency".to_string(), config.quality.concurrency.to_string());

    // Subscription settings
    settings.insert("subscription_auto_refresh_interval_mins".to_string(), config.subscription.auto_refresh_interval_mins.to_string());

    db.set_all_settings(&settings)?;
    tracing::info!("Seeded {} settings from config.toml to DB", settings.len());
    Ok(())
}

/// Writes runtime settings back to config.toml, preserving comments and formatting.
pub fn write_settings_to_config(
    settings: &std::collections::HashMap<String, String>,
    config_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read {config_path}: {e}"))?;
    let mut doc = content.parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("Failed to parse TOML: {e}"))?;

    for (key, value) in settings {
        match key.as_str() {
            "admin_password" => doc["server"]["admin_password"] = toml_edit::value(value.as_str()),
            "min_trust_level" => doc["server"]["min_trust_level"] = toml_edit::value(value.parse::<i64>().unwrap_or(1)),
            "allow_registration" => doc["server"]["allow_registration"] = toml_edit::value(value == "true"),
            "enable_oauth" => doc["server"]["enable_oauth"] = toml_edit::value(value == "true"),
            "oauth_client_id" => doc["oauth"]["client_id"] = toml_edit::value(value.as_str()),
            "oauth_client_secret" => doc["oauth"]["client_secret"] = toml_edit::value(value.as_str()),
            "oauth_redirect_uri" => doc["oauth"]["redirect_uri"] = toml_edit::value(value.as_str()),
            "singbox_api_secret" => doc["singbox"]["api_secret"] = toml_edit::value(value.as_str()),
            "validation_url" => doc["validation"]["url"] = toml_edit::value(value.as_str()),
            "validation_timeout_secs" => doc["validation"]["timeout_secs"] = toml_edit::value(value.parse::<i64>().unwrap_or(10)),
            "validation_concurrency" => doc["validation"]["concurrency"] = toml_edit::value(value.parse::<i64>().unwrap_or(50)),
            "validation_interval_mins" => doc["validation"]["interval_mins"] = toml_edit::value(value.parse::<i64>().unwrap_or(30)),
            "validation_error_threshold" => doc["validation"]["error_threshold"] = toml_edit::value(value.parse::<i64>().unwrap_or(10)),
            "validation_batch_size" => doc["validation"]["batch_size"] = toml_edit::value(value.parse::<i64>().unwrap_or(30)),
            "quality_interval_mins" => doc["quality"]["interval_mins"] = toml_edit::value(value.parse::<i64>().unwrap_or(120)),
            "quality_concurrency" => doc["quality"]["concurrency"] = toml_edit::value(value.parse::<i64>().unwrap_or(10)),
            "subscription_auto_refresh_interval_mins" => doc["subscription"]["auto_refresh_interval_mins"] = toml_edit::value(value.parse::<i64>().unwrap_or(0)),
            _ => { tracing::warn!("Unknown settings key for config writeback: {key}"); }
        }
    }

    std::fs::write(config_path, doc.to_string())
        .map_err(|e| format!("Failed to write {config_path}: {e}"))?;
    tracing::info!("Settings written back to {config_path}");
    Ok(())
}
