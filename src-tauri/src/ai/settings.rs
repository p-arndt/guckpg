use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Ollama,
    OpenaiResponses,
    OpenaiCompatible,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthMode {
    None,
    #[default]
    Bearer,
    Basic,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

impl BasicAuth {
    pub(super) fn validate(&self) -> AppResult<()> {
        if self.username.is_empty()
            || self.username.contains(':')
            || self.username.chars().any(char::is_control)
            || self.password.chars().any(char::is_control)
            || self.username.len() > 1024
            || self.password.len() > 16_384
        {
            return Err(AppError::msg(
                "Invalid Basic Auth credentials: enter a username without colons or control characters.",
            ));
        }
        Ok(())
    }
}

pub enum RequestAuth {
    None,
    Bearer(String),
    Basic(BasicAuth),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    pub provider: ProviderKind,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub auth_mode: AuthMode,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: ProviderKind::Ollama,
            base_url: "http://localhost:11434".into(),
            model: String::new(),
            auth_mode: AuthMode::Bearer,
        }
    }
}

impl AiConfig {
    pub fn validate(&self) -> AppResult<()> {
        self.request_url()?;
        if self.model.trim().is_empty() || self.model.len() > 512 {
            return Err(AppError::msg(
                "Enter an AI model name (at most 512 characters).",
            ));
        }
        Ok(())
    }

    pub fn url(&self) -> AppResult<reqwest::Url> {
        if self.base_url.len() > 4096
            || self.base_url.split_once("://").is_some_and(|(_, rest)| {
                rest.split(['/', '?', '#'])
                    .next()
                    .is_some_and(|authority| authority.contains('@'))
            })
        {
            return Err(AppError::msg("Invalid AI provider URL."));
        }
        let url = reqwest::Url::parse(&self.base_url)
            .map_err(|_| AppError::msg("Invalid AI provider URL."))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AppError::msg(
                "AI provider URL must use HTTP or HTTPS without credentials, query, or fragment.",
            ));
        }
        Ok(url)
    }

    /// The URL requests may be sent to. Credentials must not travel in clear text, so
    /// plain HTTP is accepted only for endpoints on this machine. Stored configs are not
    /// rejected on load, so the user can still open and correct them.
    pub fn request_url(&self) -> AppResult<reqwest::Url> {
        let url = self.url()?;
        if url.scheme() == "http" && !is_loopback(&url) {
            return Err(AppError::msg(
                "AI provider URL must use HTTPS unless it points to this computer (localhost).",
            ));
        }
        Ok(url)
    }

    fn account(&self) -> AppResult<String> {
        let endpoint = self.url()?.to_string();
        Ok(format!(
            "ai:{}:{}",
            serde_json::to_string(&self.provider)?,
            endpoint.trim_end_matches('/')
        ))
    }
}

fn is_loopback(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub config: AiConfig,
    pub has_api_key: bool,
    pub has_basic_auth: bool,
}

pub struct SettingsStore {
    path: PathBuf,
    save_lock: Mutex<()>,
}

impl SettingsStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("ai.json"),
            save_lock: Mutex::new(()),
        }
    }

    fn config(&self) -> AppResult<AiConfig> {
        match std::fs::File::open(&self.path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(16_385)
                    .read_to_end(&mut bytes)
                    .map_err(|_| AppError::msg("Could not read AI settings."))?;
                if bytes.len() > 16_384 {
                    return Err(AppError::msg("AI settings file is too large."));
                }
                let config: AiConfig = serde_json::from_slice(&bytes)
                    .map_err(|_| AppError::msg("Invalid AI settings file."))?;
                config.url()?;
                Ok(config)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(AiConfig::default()),
            Err(_) => Err(AppError::msg("Could not read AI settings.")),
        }
    }

    fn key(config: &AiConfig) -> AppResult<Option<String>> {
        Self::read_secret(&config.account()?)
    }

    fn read_secret(account: &str) -> AppResult<Option<String>> {
        crate::vault::get(account)
            .map_err(|_| AppError::msg("Could not read AI credentials from the keychain."))
    }

    pub fn load(&self) -> AppResult<AiSettings> {
        let _guard = self
            .save_lock
            .lock()
            .map_err(|_| AppError::msg("AI settings unavailable."))?;
        let config = self.config()?;
        let has_api_key = Self::key(&config)?.is_some();
        let has_basic_auth = Self::read_secret(&Self::basic_account(&config)?)?.is_some();
        Ok(AiSettings {
            config,
            has_api_key,
            has_basic_auth,
        })
    }

    fn basic_account(config: &AiConfig) -> AppResult<String> {
        Ok(format!("basic:{}", config.account()?))
    }

    pub fn auth_for(&self, config: &AiConfig) -> AppResult<RequestAuth> {
        match config.auth_mode {
            AuthMode::None => Ok(RequestAuth::None),
            AuthMode::Bearer => {
                Ok(Self::key(config)?.map_or(RequestAuth::None, RequestAuth::Bearer))
            }
            AuthMode::Basic => {
                let value = Self::read_secret(&Self::basic_account(config)?)?.ok_or_else(|| {
                    AppError::msg("Save Basic Auth credentials for this endpoint first.")
                })?;
                let basic: BasicAuth = serde_json::from_str(&value)
                    .map_err(|_| AppError::msg("Invalid Basic Auth keychain entry."))?;
                basic.validate()?;
                Ok(RequestAuth::Basic(basic))
            }
        }
    }

    pub fn save(
        &self,
        config: AiConfig,
        api_key: Option<String>,
        basic_auth: Option<BasicAuth>,
    ) -> AppResult<AiSettings> {
        config.validate()?;
        if let Some(basic) = &basic_auth
            && !(basic.username.is_empty() && basic.password.is_empty())
        {
            basic.validate()?;
        }
        let _guard = self
            .save_lock
            .lock()
            .map_err(|_| AppError::msg("AI settings unavailable."))?;
        if let Some(key) = api_key {
            if key.len() > 16_384 || key.contains(['\r', '\n']) {
                return Err(AppError::msg("Invalid AI API key."));
            }
            let account = config.account()?;
            if key.is_empty() {
                crate::vault::delete(&account)?;
            } else {
                crate::vault::set(&account, &key)?;
            }
        }
        if let Some(basic) = basic_auth {
            let account = Self::basic_account(&config)?;
            if basic.username.is_empty() && basic.password.is_empty() {
                crate::vault::delete(&account)?;
            } else {
                crate::vault::set(&account, &serde_json::to_string(&basic)?)?;
            }
        }
        let dir = self
            .path
            .parent()
            .ok_or_else(|| AppError::msg("Invalid AI settings path."))?;
        std::fs::create_dir_all(dir)?;
        self.write_config(&config)?;
        let has_api_key = Self::key(&config)?.is_some();
        let has_basic_auth = Self::read_secret(&Self::basic_account(&config)?)?.is_some();
        Ok(AiSettings {
            config,
            has_api_key,
            has_basic_auth,
        })
    }

    fn write_config(&self, config: &AiConfig) -> AppResult<()> {
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temporary, serde_json::to_vec_pretty(&config)?)?;
        if let Err(error) = std::fs::rename(&temporary, &self.path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_configs_keep_bearer_auth_and_basic_secrets_are_separate() {
        let mut config: AiConfig = serde_json::from_str(
            r#"{"provider":"ollama","baseUrl":"http://localhost:11434","model":"local"}"#,
        )
        .unwrap();
        assert_eq!(config.auth_mode, AuthMode::Bearer);
        let basic_account = SettingsStore::basic_account(&config).unwrap();
        assert_ne!(basic_account, config.account().unwrap());
        config.auth_mode = AuthMode::Basic;
        assert_eq!(
            basic_account,
            SettingsStore::basic_account(&config).unwrap()
        );
        config.base_url = "https://other.example".into();
        assert_ne!(
            basic_account,
            SettingsStore::basic_account(&config).unwrap()
        );
        let serialized = serde_json::to_value(&config).unwrap();
        assert_eq!(serialized["authMode"], "basic");
        assert!(serialized.get("username").is_none());
        assert!(serialized.get("password").is_none());
    }

    #[test]
    fn basic_auth_validates_username_without_exposing_credentials() {
        let mut basic = BasicAuth {
            username: "proxy-user".into(),
            password: "test:password".into(),
        };
        assert!(basic.validate().is_ok());
        for username in ["", "invalid:user", "user\nname"] {
            basic.username = username.into();
            let error = basic.validate().unwrap_err().to_string();
            assert!(!error.contains(&basic.password));
        }
        basic.username = "proxy-user".into();
        basic.password = "bad\rpassword".into();
        assert!(basic.validate().is_err());
        basic.password.clear();
        assert!(basic.validate().is_ok());
    }

    #[test]
    fn validates_endpoint_and_model_without_echoing_input() {
        let mut config = AiConfig::default();
        assert!(config.validate().is_err());
        config.model = "local-model".into();
        assert!(config.validate().is_ok());
        for url in [
            "https://user:secret@example.com",
            "https://example.com?q=secret",
            "file:///tmp/model",
            "https://example.com/#secret",
        ] {
            config.base_url = url.into();
            let error = config.validate().unwrap_err().to_string();
            assert!(!error.contains("secret"));
        }
    }

    #[test]
    fn plain_http_is_limited_to_loopback_hosts() {
        let mut config = AiConfig {
            model: "local-model".into(),
            ..Default::default()
        };
        for url in [
            "http://localhost:11434",
            "http://LOCALHOST:11434/",
            "http://127.0.0.1:8080/v1",
            "http://127.1.2.3",
            "http://[::1]:11434",
            "https://api.example.com/v1",
            "https://192.168.1.10:11434",
        ] {
            config.base_url = url.into();
            assert!(config.validate().is_ok(), "{url}");
        }
        for url in [
            "http://api.example.com/v1",
            "http://192.168.1.10:11434",
            "http://[fe80::1]:11434",
            "http://localhost.example.com",
            "http://0.0.0.0:11434",
        ] {
            config.base_url = url.into();
            assert!(config.validate().is_err(), "{url}");
            // Remote HTTP configs stay loadable so the user can correct them.
            assert!(config.url().is_ok(), "{url}");
        }
    }

    #[test]
    fn key_accounts_are_bound_to_provider_and_endpoint() {
        let mut config = AiConfig::default();
        let original = config.account().unwrap();
        config.base_url.push('/');
        assert_eq!(original, config.account().unwrap());
        config.provider = ProviderKind::OpenaiCompatible;
        assert_ne!(original, config.account().unwrap());
        config.provider = ProviderKind::Ollama;
        config.base_url = "http://another-host:11434".into();
        assert_ne!(original, config.account().unwrap());
    }

    #[test]
    fn config_json_contains_no_key_and_has_frontend_shape() {
        let value = serde_json::to_value(AiConfig {
            provider: ProviderKind::OpenaiResponses,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(value["provider"], "openaiResponses");
        assert!(value.get("baseUrl").is_some());
        assert!(value.get("apiKey").is_none());
    }

    #[test]
    fn config_persistence_is_atomic_and_rejects_corrupt_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        assert_eq!(store.config().unwrap().model, "");
        let config = AiConfig {
            model: "saved-model".into(),
            ..Default::default()
        };
        store.write_config(&config).unwrap();
        assert_eq!(store.config().unwrap().model, "saved-model");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        std::fs::write(&store.path, b"private-invalid-value").unwrap();
        assert!(
            !store
                .config()
                .unwrap_err()
                .to_string()
                .contains("private-invalid-value")
        );
        std::fs::write(&store.path, vec![b' '; 16_385]).unwrap();
        assert!(store.config().is_err());
    }
}
