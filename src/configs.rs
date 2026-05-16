use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

// ── Config struct ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configs {
    pub arduino_cli_path: String,
    pub arduino_data_path: String,
    pub arduino_downloads_path: String,
    pub arduino_sketch_path: String,
    pub auto_start: bool,
    pub arduino_preferences_path: String,
    pub arduino_sketch_path_from_preferences: bool,
    pub additional_urls_from_preferences: bool,
}

fn arduino_cli_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "arduino-cli.exe"
    } else if cfg!(target_os = "macos") {
        "arduino-cli"
    } else {
        "arduino-cli-ubuntu-x64"
    }
}

pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join("AppData")
        .join("Local")
        .join("FlowcodeAgent")
}

pub fn config_file() -> PathBuf {
    config_dir().join("configs.json")
}

impl Default for Configs {
    fn default() -> Self {
        let home = dirs::home_dir().expect("no home dir");
        let arduino15 = home.join("AppData").join("Local").join("Arduino15");
        Self {
            arduino_cli_path: home
                .join("AppData")
                .join("Local")
                .join("Programs")
                .join("Arduino IDE")
                .join("resources")
                .join("app")
                .join("lib")
                .join("backend")
                .join("resources")
                .join(arduino_cli_name())
                .to_string_lossy()
                .to_string(),
            arduino_data_path: arduino15.to_string_lossy().to_string(),
            arduino_downloads_path: arduino15.join("staging").to_string_lossy().to_string(),
            arduino_sketch_path: home
                .join("Documents")
                .join("Arduino")
                .to_string_lossy()
                .to_string(),
            auto_start: true,
            arduino_preferences_path: arduino15
                .join("preferences.txt")
                .to_string_lossy()
                .to_string(),
            arduino_sketch_path_from_preferences: true,
            additional_urls_from_preferences: true,
        }
    }
}

// ── preferences.txt parser ─────────────────────────────────────────────────

fn read_prefs(path: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            if let Some(eq) = line.find('=') {
                let k = line[..eq].trim().to_string();
                let v = line[eq + 1..].trim().to_string();
                map.insert(k, v);
            }
        }
    }
    map
}

// ── global config store ────────────────────────────────────────────────────

static CONFIGS: OnceLock<RwLock<Configs>> = OnceLock::new();

fn configs_lock() -> &'static RwLock<Configs> {
    CONFIGS.get_or_init(|| RwLock::new(Configs::default()))
}

pub fn load_configs() {
    let dir = config_dir();
    let file = config_file();

    if !dir.exists() {
        fs::create_dir_all(&dir).ok();
    }

    let mut configs: Configs = if file.exists() {
        let raw = fs::read_to_string(&file).unwrap_or_default();
        serde_json::from_str(&raw).unwrap_or_default()
    } else {
        let defaults = Configs::default();
        if let Ok(json) = serde_json::to_string_pretty(&defaults) {
            fs::write(&file, json).ok();
        }
        defaults
    };

    if configs.arduino_sketch_path_from_preferences {
        let prefs = read_prefs(&configs.arduino_preferences_path);
        if let Some(p) = prefs.get("sketchbook.path") {
            configs.arduino_sketch_path = p.clone();
        }
    }

    *configs_lock().write().unwrap() = configs;
}

pub fn get_configs() -> Configs {
    configs_lock().read().unwrap().clone()
}

pub fn get_additional_urls_from_preferences() -> Vec<String> {
    let configs = get_configs();
    if !configs.additional_urls_from_preferences {
        return vec![];
    }
    let prefs = read_prefs(&configs.arduino_preferences_path);
    prefs
        .get("boardsmanager.additional.urls")
        .map(|s| {
            s.split(',')
                .map(|u| u.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}
