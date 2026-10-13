//! Persisted settings, workspaces, and the sampled history behind the
//! sparkline.
//!
//! A workspace is one Go API key with its own label and its own sample
//! history. Usage limits are per workspace, so nothing here ever combines
//! them; the file simply holds a list and remembers which one is active.
//!
//! Files written by the single-key version of the app are migrated on load:
//! the old `settings.apiKey` becomes the first workspace, and the old
//! top-level `history` moves into it.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn default_poll() -> u64 {
    60
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Id of the workspace the panel shows.
    #[serde(default)]
    pub active_key_id: String,
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
    #[serde(default = "default_true")]
    pub show_percent_in_menu_bar: bool,

    /// Legacy single-key field, folded into `keys` on load and never written.
    #[serde(default, rename = "apiKey", skip_serializing)]
    legacy_api_key: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            active_key_id: String::new(),
            poll_seconds: default_poll(),
            show_percent_in_menu_bar: true,
            legacy_api_key: None,
        }
    }
}

impl Settings {
    /// Keeps a hand-edited config file from hammering the endpoint.
    pub fn normalized(mut self) -> Self {
        self.poll_seconds = self.poll_seconds.clamp(15, 3600);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// unix seconds
    pub t: i64,
    /// highest percent across the three windows at that moment
    pub peak: f64,
}

/// One Go API key and the readings taken with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    #[serde(default)]
    pub id: String,
    /// Shown in the panel and the tray; renamed by the user.
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub history: Vec<Sample>,
}

impl Workspace {
    /// Last few characters of a key, for labels and previews — never the whole
    /// secret. Empty when there is no key.
    pub fn key_tail(api_key: &str) -> String {
        let trimmed = api_key.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        let tail: String = trimmed
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("…{tail}")
    }

    /// What to call a workspace nobody has named yet.
    pub fn default_label(api_key: &str) -> String {
        let tail = Self::key_tail(api_key);
        if tail.is_empty() {
            "New workspace".to_string()
        } else {
            tail
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.trim().is_empty()
    }
}

/// The two preferences the panel owns, separate from the workspace list so a
/// settings save can never blank out which workspace is active.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub poll_seconds: u64,
    pub show_percent_in_menu_bar: bool,
}

impl From<&Settings> for Preferences {
    fn from(settings: &Settings) -> Self {
        Self {
            poll_seconds: settings.poll_seconds,
            show_percent_in_menu_bar: settings.show_percent_in_menu_bar,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub settings: Settings,
    pub keys: Vec<Workspace>,

    /// Legacy top-level history, moved into the first workspace on load.
    #[serde(default, rename = "history", skip_serializing_if = "Option::is_none")]
    legacy_history: Option<Vec<Sample>>,
}

/// Six hours of samples; that is the window the sparkline claims.
const HISTORY_SECONDS: i64 = 6 * 3600;
const HISTORY_MAX: usize = 1440;

impl Config {
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut config: Config = serde_json::from_str(&raw).unwrap_or_default();
        config.migrate();
        config.trim_histories();
        config
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        restrict_permissions(path);
        Ok(())
    }

    /// Folds the single-key layout into the workspace list and re-establishes
    /// the invariants: unique non-empty ids, a label on every workspace, and
    /// an active id that exists.
    fn migrate(&mut self) {
        self.settings = self.settings.clone().normalized();

        let legacy_key = self
            .settings
            .legacy_api_key
            .take()
            .unwrap_or_default()
            .trim()
            .to_string();
        let legacy_history = self.legacy_history.take().unwrap_or_default();

        if !legacy_key.is_empty() {
            match self.keys.iter_mut().find(|key| key.api_key.trim() == legacy_key) {
                Some(existing) if existing.history.is_empty() => existing.history = legacy_history,
                Some(_) => {}
                None => self.keys.push(Workspace {
                    id: String::new(),
                    label: String::new(),
                    api_key: legacy_key,
                    history: legacy_history,
                }),
            }
        } else if let Some(first) = self.keys.first_mut() {
            if first.history.is_empty() {
                first.history = legacy_history;
            }
        }

        // Ids: fill gaps, and drop accidental duplicates.
        let mut seen: Vec<String> = Vec::new();
        let mut fresh = 0;
        for key in &mut self.keys {
            let mut id = key.id.trim().to_string();
            if id.is_empty() || seen.contains(&id) {
                fresh += 1;
                id = format!("k{}-{}", now_secs(), fresh);
            }
            seen.push(id.clone());
            key.id = id;
            if key.label.trim().is_empty() {
                key.label = Workspace::default_label(&key.api_key);
            }
        }

        // Exactly one active workspace, and only if a workspace exists.
        if !self.keys.iter().any(|key| key.id == self.settings.active_key_id) {
            self.settings.active_key_id = self
                .keys
                .first()
                .map(|key| key.id.clone())
                .unwrap_or_default();
        }
    }

    /// An id that is not in use yet.
    pub fn next_id(&self) -> String {
        let mut candidate = format!("k{}", now_secs());
        let mut suffix = 0;
        while self.keys.iter().any(|key| key.id == candidate) {
            suffix += 1;
            candidate = format!("k{}-{}", now_secs(), suffix);
        }
        candidate
    }

    pub fn active(&self) -> Option<&Workspace> {
        self.keys
            .iter()
            .find(|key| key.id == self.settings.active_key_id)
    }


    /// Appends one reading to a workspace's history.
    pub fn record(&mut self, key_id: &str, peak: f64) {
        let Some(workspace) = self.keys.iter_mut().find(|key| key.id == key_id) else {
            return;
        };
        let t = now_secs();
        // One sample per minute is plenty for a six hour graph.
        if let Some(last) = workspace.history.last() {
            if t - last.t < 55 {
                workspace.history.pop();
            }
        }
        workspace.history.push(Sample { t, peak });
        trim_history(&mut workspace.history);
    }

    fn trim_histories(&mut self) {
        for workspace in &mut self.keys {
            trim_history(&mut workspace.history);
        }
    }
}

fn trim_history(history: &mut Vec<Sample>) {
    let cutoff = now_secs() - HISTORY_SECONDS;
    history.retain(|sample| sample.t >= cutoff);
    if history.len() > HISTORY_MAX {
        let drop = history.len() - HISTORY_MAX;
        history.drain(0..drop);
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

/// `~/Library/Application Support/<bundle id>/config.json`
pub fn config_path(dir: PathBuf) -> PathBuf {
    dir.join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_from(json: &str) -> Config {
        let mut config: Config = serde_json::from_str(json).expect("parse");
        config.migrate();
        config
    }

    #[test]
    fn migrates_a_single_key_file_into_one_workspace() {
        let config = config_from(
            r#"{
                "settings": { "apiKey": "oc_personal_key_1234", "pollSeconds": 120, "showPercentInMenuBar": false },
                "history": [ { "t": 100, "peak": 12 } ]
            }"#,
        );

        assert_eq!(config.keys.len(), 1, "the old key becomes one workspace");
        assert_eq!(config.keys[0].api_key, "oc_personal_key_1234");
        assert_eq!(config.keys[0].history.len(), 1, "history moves with it");
        assert_eq!(config.settings.poll_seconds, 120, "other settings survive");
        assert!(!config.settings.show_percent_in_menu_bar);
        assert_eq!(config.settings.active_key_id, config.keys[0].id);
        assert_eq!(config.keys[0].label, "…1234", "labelled from the key tail");
    }

    #[test]
    fn a_migrated_file_no_longer_carries_the_legacy_fields() {
        let config = config_from(r#"{ "settings": { "apiKey": "oc_key_abcd" } }"#);
        let json = serde_json::to_string(&config).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("reparse");

        // The key now lives in the workspace list, not in settings.
        assert!(
            value["settings"].get("apiKey").is_none(),
            "settings must not carry a key: {json}"
        );
        assert!(
            value["keys"][0].get("apiKey").is_some(),
            "the workspace holds it instead: {json}"
        );
        // The old top-level history field is gone too.
        assert!(value.get("history").is_none(), "no legacy history: {json}");
        assert!(value["keys"][0].get("history").is_some());
    }

    #[test]
    fn an_empty_config_is_valid_and_has_no_active_workspace() {
        let config = Config::default();
        assert!(config.keys.is_empty());
        assert!(config.settings.active_key_id.is_empty());
        assert!(config.active().is_none());
    }

    #[test]
    fn active_id_is_repaired_when_it_points_nowhere() {
        let config = config_from(
            r#"{ "settings": { "activeKeyId": "gone" },
                 "keys": [ { "id": "a", "apiKey": "oc_first_1111" },
                           { "id": "b", "apiKey": "oc_second_2222" } ] }"#,
        );
        assert_eq!(config.settings.active_key_id, "a", "falls back to the first");
        assert_eq!(config.keys[0].label, "…1111");
        assert_eq!(config.keys[1].label, "…2222");
    }

    #[test]
    fn duplicate_or_missing_ids_are_replaced() {
        let config = config_from(
            r#"{ "keys": [ { "id": "same", "apiKey": "oc_one_1111" },
                          { "id": "same", "apiKey": "oc_two_2222" },
                          { "apiKey": "oc_three_3333" } ] }"#,
        );
        let ids: Vec<&str> = config.keys.iter().map(|key| key.id.as_str()).collect();
        assert_eq!(ids.len(), 3);
        assert_ne!(ids[0], ids[1]);
        assert!(!ids[2].is_empty());
    }

    #[test]
    fn a_second_copy_of_the_same_key_does_not_duplicate_the_workspace() {
        let config = config_from(
            r#"{ "settings": { "apiKey": "oc_same_key_9999" },
                 "history": [ { "t": 100, "peak": 5 } ],
                 "keys": [ { "id": "a", "apiKey": "oc_same_key_9999", "history": [] } ] }"#,
        );
        assert_eq!(config.keys.len(), 1, "the key was already stored");
        assert_eq!(config.keys[0].history.len(), 1, "history still lands somewhere");
    }

    #[test]
    fn samples_land_in_their_own_workspace() {
        let mut config = config_from(
            r#"{ "keys": [ { "id": "a", "apiKey": "oc_one_1111" },
                          { "id": "b", "apiKey": "oc_two_2222" } ] }"#,
        );
        config.settings.active_key_id = "a".into();
        config.record("a", 10.0);
        config.record("b", 20.0);

        assert_eq!(config.keys[0].history.len(), 1);
        assert_eq!(config.keys[0].history[0].peak, 10.0);
        assert_eq!(config.keys[1].history.len(), 1);
        assert_eq!(config.keys[1].history[0].peak, 20.0);
    }

    #[test]
    fn new_ids_do_not_collide() {
        let mut config = Config::default();
        let first = config.next_id();
        config.keys.push(Workspace {
            id: first.clone(),
            label: "x".into(),
            api_key: "oc_x".into(),
            history: Vec::new(),
        });
        let second = config.next_id();
        assert_ne!(first, second);
    }
}
