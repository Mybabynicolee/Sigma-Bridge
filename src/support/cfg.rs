#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::net::egress::Proxy;
use crate::support::consts::{CONFIG_FILE, DATA_FILE, PROXY_FILE};
use crate::support::error::{AppError, AppResult};

const DEFAULTS: &str = r#"{
  "settings": {
    "sleep_seconds": 3600,
    "round_robin": true,
    "sticky": 3,
    "port": 1122
  }
}
"#;

#[derive(Debug, Clone)]
pub struct Config {
    value: Value,
}

impl Config {
    pub const DEFAULT_SLEEP_SECONDS: u64 = 3600;
    pub const MIN_SLEEP_SECONDS: u64 = 60;
    pub const MAX_SLEEP_SECONDS: u64 = 86_400;
    pub const DEFAULT_PORT: u16 = 1122;
    pub const DEFAULT_STICKY: u64 = 3;

    pub fn defaults() -> Config {
        Config {
            value: serde_json::from_str(DEFAULTS).unwrap_or(Value::Null),
        }
    }

    pub fn parse(text: &str) -> AppResult<Config> {
        let value: Value = serde_json::from_str(text)
            .map_err(|err| AppError::config(format!("config.json is not valid json: {}", err)))?;
        let config = Config { value };
        config.validate()?;
        Ok(config)
    }

    pub fn load(path: &Path) -> AppResult<Config> {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::parse(&text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                std::fs::write(path, DEFAULTS).map_err(|err| {
                    AppError::config(format!("config.json could not be written: {}", err))
                })?;
                Ok(Config::defaults())
            }
            Err(err) => Err(AppError::config(format!(
                "config.json could not be read: {}",
                err
            ))),
        }
    }

    pub fn sleep_seconds(&self) -> u64 {
        self.pick_u64("sleep_seconds")
            .unwrap_or(Config::DEFAULT_SLEEP_SECONDS)
            .clamp(Config::MIN_SLEEP_SECONDS, Config::MAX_SLEEP_SECONDS)
    }

    pub fn round_robin(&self) -> bool {
        self.pick_bool("round_robin").unwrap_or(true)
    }

    pub fn sticky(&self) -> u64 {
        self.pick_u64("sticky")
            .unwrap_or(Config::DEFAULT_STICKY)
            .clamp(1, 100)
    }

    pub fn port(&self) -> u16 {
        match self.pick_u64("port") {
            Some(raw) if raw > 0 && raw <= u16::MAX as u64 => raw as u16,
            _ => Config::DEFAULT_PORT,
        }
    }

    fn settings(&self) -> Option<&Value> {
        match self.value.get("settings") {
            Some(value) if value.is_object() => Some(value),
            _ => None,
        }
    }

    fn pick_u64(&self, key: &str) -> Option<u64> {
        let settings = self.settings()?;
        let found = settings.get(key)?;
        if let Some(raw) = found.as_u64() {
            return Some(raw);
        }
        found.as_str().and_then(|raw| raw.trim().parse().ok())
    }

    fn pick_bool(&self, key: &str) -> Option<bool> {
        let settings = self.settings()?;
        let found = settings.get(key)?;
        if let Some(raw) = found.as_bool() {
            return Some(raw);
        }
        found.as_str().map(|raw| {
            matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes"
            )
        })
    }

    fn validate(&self) -> AppResult<()> {
        if self.value.get("settings").is_none() {
            return Err(AppError::config("config.json needs a settings block"));
        }
        Ok(())
    }
}

pub struct Setup {
    pub root: PathBuf,
    pub config: Config,
    pub proxies: Vec<Proxy>,
}

impl Setup {
    pub fn proxy_for(&self, index: usize) -> Option<&Proxy> {
        if self.proxies.is_empty() {
            return None;
        }
        self.proxies.get(index % self.proxies.len())
    }

    pub fn data_path(&self) -> PathBuf {
        self.root.join(DATA_FILE)
    }

    pub fn proxy_path(&self) -> PathBuf {
        self.root.join(PROXY_FILE)
    }
}

pub fn load(root: &Path) -> AppResult<Setup> {
    let config = Config::load(&root.join(CONFIG_FILE))?;
    let proxies = read_proxies(&root.join(PROXY_FILE))?;

    Ok(Setup {
        root: root.to_path_buf(),
        config,
        proxies,
    })
}

pub fn read_proxies(path: &Path) -> AppResult<Vec<Proxy>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(AppError::config(format!(
                "{} could not be read: {}",
                PROXY_FILE, err
            )))
        }
    };

    let mut proxies: Vec<Proxy> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let proxy = Proxy::parse(trimmed).map_err(|err| {
            AppError::config(format!("{} line {}: {}", PROXY_FILE, index + 1, err))
        })?;
        proxies.push(proxy);
    }
    Ok(proxies)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_block_holds_four_keys_on_eight_lines() {
        assert_eq!(DEFAULTS.len(), 110);
        assert_eq!(DEFAULTS.lines().count(), 8);
    }

    #[test]
    fn the_default_block_carries_the_four_settings() {
        let config = Config::parse(DEFAULTS).unwrap();
        assert_eq!(config.sleep_seconds(), 3600);
        assert!(config.round_robin());
        assert_eq!(config.sticky(), 3);
        assert_eq!(config.port(), 1122);
        let settings = config.value.get("settings").unwrap();
        assert_eq!(settings.as_object().unwrap().len(), 4);
    }

    #[test]
    fn the_settings_are_read_from_the_file() {
        let config = Config::parse(
            r#"{"settings":{"sleep_seconds":90,"round_robin":false,"sticky":5,"port":8080}}"#,
        )
        .unwrap();
        assert_eq!(config.sleep_seconds(), 90);
        assert!(!config.round_robin());
        assert_eq!(config.sticky(), 5);
        assert_eq!(config.port(), 8080);
    }

    #[test]
    fn sleep_is_clamped_at_both_ends() {
        let config = Config::parse(r#"{"settings":{"sleep_seconds":1}}"#).unwrap();
        assert_eq!(config.sleep_seconds(), 60);
        let config = Config::parse(r#"{"settings":{"sleep_seconds":99999999}}"#).unwrap();
        assert_eq!(config.sleep_seconds(), 86_400);
    }

    #[test]
    fn a_missing_setting_falls_back_to_its_default() {
        let config = Config::parse(r#"{"settings":{}}"#).unwrap();
        assert_eq!(config.sleep_seconds(), Config::DEFAULT_SLEEP_SECONDS);
        assert!(config.round_robin());
        assert_eq!(config.sticky(), Config::DEFAULT_STICKY);
        assert_eq!(config.port(), Config::DEFAULT_PORT);
    }

    #[test]
    fn missing_settings_block_is_rejected() {
        assert!(Config::parse("{}").is_err());
    }

    #[test]
    fn an_empty_proxy_file_means_direct() {
        let dir = std::env::temp_dir().join(format!("sig-cfg-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("proxy.txt");
        std::fs::write(&path, "# one proxy per line\n").unwrap();
        assert!(read_proxies(&path).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn proxy_is_paired_by_position() {
        let setup = Setup {
            root: PathBuf::from("."),
            config: Config::defaults(),
            proxies: vec![
                Proxy::parse("socks5://127.0.0.1:1080").unwrap(),
                Proxy::parse("socks5://127.0.0.1:1081").unwrap(),
            ],
        };
        assert_eq!(setup.proxy_for(0).map(|p| p.port), Some(1080));
        assert_eq!(setup.proxy_for(1).map(|p| p.port), Some(1081));
        assert_eq!(setup.proxy_for(2).map(|p| p.port), Some(1080));
    }
}
