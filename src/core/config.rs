use std::path::PathBuf;

use crate::core::settings::{Settings, Value};
use crate::network::base::FetchOptions;

#[derive(Debug, Clone)]
pub struct Config {
    pub homepage: String,
    pub max_tabs: u32,
    pub autoload_images: bool,
    pub timeout: u64,
    pub follow_redirects: bool,
    pub url_bar_hold_ms: u64,
    pub delete_confirmation: bool,
    pub form_reset: bool,

    pub allow_protocols_v1: bool,
    pub allow_unencrypted_tcp: bool,
    pub ssh_to_tls: bool,
    pub try_encryption_against_utnp: bool,
    pub ask_new_ssh_key: bool,
    pub block_changed_ssh_key: bool,
    pub ssh_key_store: String,

    pub kb_info: String,
    pub kb_tabs: String,
    pub kb_settings: String,
    pub kb_quit: String,
    pub kb_create_tab: String,
    pub kb_delete_tab: String,
    pub kb_next_tab: String,
    pub kb_prev_tab: String,
    pub kb_back: String,
    pub kb_forward: String,
    pub kb_reload: String,
    pub kb_bookmarks: String,
    pub kb_favorite: String,
    pub kb_open_new_tab: String,
    pub kb_favorites: [String; 10],
}

impl Config {
    pub fn from_settings(settings: &Settings) -> Self {
        let mut cfg = Self::default();

        if let Some(Value::Str(s)) = settings.get("general.homepage") {
            cfg.homepage = s.clone();
        }
        if let Some(Value::Int(i)) = settings.get("general.max_tabs") {
            cfg.max_tabs = (*i).max(1) as u32;
        }
        if let Some(Value::Bool(b)) = settings.get("general.autoload_images") {
            cfg.autoload_images = *b;
        }
        if let Some(Value::Int(i)) = settings.get("network.timeout") {
            cfg.timeout = (*i).max(1) as u64;
        }
        if let Some(Value::Bool(b)) = settings.get("network.follow_redirects") {
            cfg.follow_redirects = *b;
        }
        if let Some(Value::Bool(b)) = settings.get("network.allow_protocols_v1") {
            cfg.allow_protocols_v1 = *b;
        }
        if let Some(Value::Bool(b)) =
            settings.get("network.connection.allow_unencrypted_tcp")
        {
            cfg.allow_unencrypted_tcp = *b;
        }
        if let Some(Value::Bool(b)) = settings.get("network.connection.ssh_to_tls") {
            cfg.ssh_to_tls = *b;
        }
        if let Some(Value::Bool(b)) =
            settings.get("network.connection.try_encryption_against_utnp")
        {
            cfg.try_encryption_against_utnp = *b;
        }
        if let Some(Value::Bool(b)) = settings.get("network.ssh.ask_new_ssh_key") {
            cfg.ask_new_ssh_key = *b;
        }
        if let Some(Value::Bool(b)) = settings.get("network.ssh.block_changed_ssh_key") {
            cfg.block_changed_ssh_key = *b;
        }
        if let Some(Value::Str(s)) = settings.get("network.ssh.ssh_key_store") {
            cfg.ssh_key_store = s.clone();
        }
        if let Some(Value::Float(f)) = settings.get("appearance.layout.url_bar_hold") {
            cfg.url_bar_hold_ms = (f.max(0.0) * 1000.0) as u64;
        }
        if let Some(Value::Bool(b)) = settings.get("general.keybind.delete_hint") {
            cfg.delete_confirmation = *b;
        }

        macro_rules! get_kb {
            ($key:literal, $field:ident) => {
                if let Some(Value::Str(s)) = settings.get($key) {
                    cfg.$field = s.clone();
                }
            };
        }

        get_kb!("general.keybind.info", kb_info);
        get_kb!("general.keybind.tabs", kb_tabs);
        get_kb!("general.keybind.settings", kb_settings);
        get_kb!("general.keybind.quit", kb_quit);
        get_kb!("general.keybind.create_tab", kb_create_tab);
        get_kb!("general.keybind.delete_tab", kb_delete_tab);
        get_kb!("general.keybind.next_tab", kb_next_tab);
        get_kb!("general.keybind.prev_tab", kb_prev_tab);
        get_kb!("general.keybind.back", kb_back);
        get_kb!("general.keybind.forward", kb_forward);
        get_kb!("general.keybind.reload", kb_reload);
        get_kb!("general.keybind.bookmarks", kb_bookmarks);
        get_kb!("general.keybind.favorite", kb_favorite);
        get_kb!("general.keybind.open_new_tab", kb_open_new_tab);

        for (i, key) in [
            "general.keybind.favorite_1",
            "general.keybind.favorite_2",
            "general.keybind.favorite_3",
            "general.keybind.favorite_4",
            "general.keybind.favorite_5",
            "general.keybind.favorite_6",
            "general.keybind.favorite_7",
            "general.keybind.favorite_8",
            "general.keybind.favorite_9",
            "general.keybind.favorite_10",
        ]
        .iter()
        .enumerate()
        {
            if let Some(Value::Str(s)) = settings.get(key) {
                cfg.kb_favorites[i] = s.clone();
            }
        }

        cfg
    }

    pub fn fetch_options(&self) -> FetchOptions {
        FetchOptions {
            follow_redirects: self.follow_redirects,
            max_redirects: 10,
            timeout_secs: self.timeout,
            allow_v1: self.allow_protocols_v1,
            allow_unencrypted_tcp: self.allow_unencrypted_tcp,
            try_encryption_against_utnp: self.try_encryption_against_utnp,
            ssh_key_store: self.resolved_ssh_key_store(),
        }
    }

    fn resolved_ssh_key_store(&self) -> Option<PathBuf> {
        let raw = self.ssh_key_store.trim();

        if raw.is_empty() {
            return Settings::ssh_key_store_path();
        }

        Some(expand_path(raw))
    }
}

fn expand_path(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }

    if let Some(rest) = raw.strip_prefix("$XDG_CONFIG_HOME/") {
        if let Some(config) = dirs::config_dir() {
            return config.join(rest);
        }
    }

    if let Some(rest) = raw.strip_prefix("$HOME/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }

    PathBuf::from(raw)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            homepage: "tmutp://localhost/".to_string(),
            max_tabs: 20,
            autoload_images: true,
            timeout: 30,
            follow_redirects: true,
            url_bar_hold_ms: 300,
            delete_confirmation: true,
            form_reset: false,

            allow_protocols_v1: false,
            allow_unencrypted_tcp: false,
            ssh_to_tls: false,
            try_encryption_against_utnp: false,
            ask_new_ssh_key: true,
            block_changed_ssh_key: false,
            ssh_key_store: "$XDG_CONFIG_HOME/tbrowser/ssh/known_hosts".to_string(),

            kb_info: "i".to_string(),
            kb_tabs: "t".to_string(),
            kb_settings: "s".to_string(),
            kb_quit: "q".to_string(),
            kb_create_tab: "c".to_string(),
            kb_delete_tab: "d".to_string(),
            kb_next_tab: "n".to_string(),
            kb_prev_tab: "p".to_string(),
            kb_back: "b".to_string(),
            kb_forward: "f".to_string(),
            kb_reload: "r".to_string(),
            kb_bookmarks: "m".to_string(),
            kb_favorite: "v".to_string(),
            kb_open_new_tab: "Ctrl+Enter".to_string(),
            kb_favorites: [
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
                "5".to_string(),
                "6".to_string(),
                "7".to_string(),
                "8".to_string(),
                "9".to_string(),
                "0".to_string(),
            ],
        }
    }
}
