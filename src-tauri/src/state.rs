//! Runtime state, split by responsibility.
//!
//! [`ConfigStore`] owns everything that is persisted — settings, workspaces and
//! their sample histories. [`Meter`] owns what is currently known about each
//! workspace endpoint. Callers talk to [`AppState`], which composes the two and
//! keeps the locking and the in-flight guard inside, so no command or loop has
//! to juggle a mutex.
//!
//! The fetch itself is injected (the poll loop decides when), which is why this
//! module has no idea that a network exists.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::Notify;

use crate::config::{now_secs, Config, Preferences, Sample, Settings, Workspace};
use crate::usage::{Usage, UsageError};

/// Settings, workspaces and history on disk. Every write goes through here.
pub struct ConfigStore {
    config: Mutex<Config>,
    path: PathBuf,
}

impl ConfigStore {
    pub fn open(path: PathBuf) -> Self {
        let config = Config::load(&path);
        Self {
            config: Mutex::new(config),
            path,
        }
    }

    fn read<T>(&self, read: impl FnOnce(&Config) -> T) -> Option<T> {
        self.config.lock().ok().map(|config| read(&config))
    }

    /// Locks, changes, and persists in one step; returns what the change
    /// produced.
    fn edit<T>(&self, change: impl FnOnce(&mut Config) -> T) -> std::io::Result<T> {
        let mut config = self.config.lock().map_err(|_| poisoned("settings"))?;
        let outcome = change(&mut config);
        config.save(&self.path)?;
        Ok(outcome)
    }

    /* ---------------------------------------------------------- readers */

    pub fn settings(&self) -> Settings {
        self.read(|config| config.settings.clone()).unwrap_or_default()
    }

    pub fn workspaces(&self) -> Vec<Workspace> {
        self.read(|config| config.keys.clone()).unwrap_or_default()
    }

    pub fn active_id(&self) -> String {
        self.settings().active_key_id
    }

    pub fn active(&self) -> Option<Workspace> {
        self.read(|config| config.active().cloned()).flatten()
    }

    /// The secret of the active workspace, or an empty string.
    pub fn active_key(&self) -> String {
        self.active().map(|key| key.api_key).unwrap_or_default()
    }

    pub fn is_configured(&self) -> bool {
        self.active().map(|key| key.is_configured()).unwrap_or(false)
    }

    /// Sample history of the active workspace.
    pub fn history(&self) -> Vec<Sample> {
        self.active().map(|key| key.history).unwrap_or_default()
    }

    /* ---------------------------------------------------------- writers */

    pub fn set_preferences(&self, preferences: Preferences) -> std::io::Result<Settings> {
        self.edit(|config| {
            config.settings.poll_seconds = preferences.poll_seconds;
            config.settings.show_percent_in_menu_bar = preferences.show_percent_in_menu_bar;
            config.settings.clone()
        })
    }

    /// Adds a workspace. An empty key is allowed: that is how "Add workspace"
    /// starts, and the settings screen asks for the key next.
    pub fn add_workspace(
        &self,
        label: Option<String>,
        api_key: &str,
        make_active: bool,
    ) -> std::io::Result<Settings> {
        let api_key = api_key.trim().to_string();
        self.edit(|config| {
            let id = config.next_id();
            let label = match label.map(|l| l.trim().to_string()) {
                Some(label) if !label.is_empty() => label,
                _ => Workspace::default_label(&api_key),
            };
            config.keys.push(Workspace {
                id: id.clone(),
                label,
                api_key,
                history: Vec::new(),
            });
            if make_active {
                config.settings.active_key_id = id;
            }
            config.settings.clone()
        })
    }

    /// Replaces a workspace's key. A blank key leaves it without one.
    pub fn set_key(&self, id: &str, api_key: &str) -> std::io::Result<Settings> {
        let api_key = api_key.trim().to_string();
        self.edit(|config| {
            if let Some(workspace) = config.keys.iter_mut().find(|key| key.id == id) {
                workspace.api_key = api_key;
                if workspace.label.trim().is_empty() {
                    workspace.label = Workspace::default_label(&workspace.api_key);
                }
            }
            config.settings.clone()
        })
    }

    pub fn rename(&self, id: &str, label: &str) -> std::io::Result<Settings> {
        let label = label.trim().to_string();
        self.edit(|config| {
            if let Some(workspace) = config.keys.iter_mut().find(|key| key.id == id) {
                if !label.is_empty() {
                    workspace.label = label;
                }
            }
            config.settings.clone()
        })
    }

    /// Removes a workspace and its history. The active id is repaired by the
    /// migration rules, so the app is never left pointing at nothing.
    pub fn remove(&self, id: &str) -> std::io::Result<Settings> {
        self.edit(|config| {
            config.keys.retain(|key| key.id != id);
            if config.settings.active_key_id == id {
                config.settings.active_key_id = config
                    .keys
                    .first()
                    .map(|key| key.id.clone())
                    .unwrap_or_default();
            }
            config.settings.clone()
        })
    }

    pub fn select(&self, id: &str) -> std::io::Result<Settings> {
        self.edit(|config| {
            if config.keys.iter().any(|key| key.id == id) {
                config.settings.active_key_id = id.to_string();
            }
            config.settings.clone()
        })
    }

    /// Appends one reading to a workspace's history and persists it.
    pub fn record(&self, key_id: &str, peak: f64) {
        if let Ok(mut config) = self.config.lock() {
            config.record(key_id, peak);
            let _ = config.save(&self.path);
        }
    }
}

/// The last thing one workspace's endpoint told us, plus whether a request is
/// out for it.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub usage: Option<Usage>,
    pub error: Option<UsageError>,
    pub fetched_at: Option<i64>,
    pub stale: bool,
    pub loading: bool,
}

/// Per-workspace readings, keyed by workspace id.
pub struct Meter {
    snapshots: Mutex<HashMap<String, Snapshot>>,
    inflight: Mutex<HashSet<String>>,
    /// Woken by a manual "check now" to cut the poll interval short.
    notify: Notify,
}

impl Meter {
    pub fn new() -> Self {
        Self {
            snapshots: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashSet::new()),
            notify: Notify::new(),
        }
    }

    pub fn snapshot(&self, key_id: &str) -> Snapshot {
        self.snapshots
            .lock()
            .ok()
            .and_then(|map| map.get(key_id).cloned())
            .unwrap_or_default()
    }

    /// Gives a workspace a starting state before anything has been fetched.
    pub fn seed(&self, key_id: &str, error: Option<UsageError>) {
        if let Ok(mut map) = self.snapshots.lock() {
            let entry = map.entry(key_id.to_string()).or_default();
            if entry.error.is_none() {
                entry.error = error;
            }
        }
    }

    fn claim(&self, key_id: &str) -> bool {
        self.inflight
            .lock()
            .map(|mut set| set.insert(key_id.to_string()))
            .unwrap_or(false)
    }

    fn release(&self, key_id: &str) {
        if let Ok(mut set) = self.inflight.lock() {
            set.remove(key_id);
        }
    }

    fn mark_loading(&self, key_id: &str, configured: bool) {
        if let Ok(mut map) = self.snapshots.lock() {
            let entry = map.entry(key_id.to_string()).or_default();
            entry.loading = configured;
            if !configured {
                entry.usage = None;
                entry.error = Some(UsageError::unconfigured());
                entry.stale = false;
            }
        }
    }

    /// Stores the outcome and returns the peak worth recording, if any.
    fn settle(&self, key_id: &str, result: Result<Usage, UsageError>) -> Option<f64> {
        let mut peak = None;
        if let Ok(mut map) = self.snapshots.lock() {
            let entry = map.entry(key_id.to_string()).or_default();
            entry.loading = false;
            match result {
                Ok(usage) => {
                    peak = Some(usage.peak());
                    entry.error = None;
                    entry.usage = Some(usage);
                    entry.fetched_at = Some(now_secs());
                    entry.stale = false;
                }
                Err(error) => {
                    // Keep the last good numbers on screen, flagged as stale.
                    entry.stale = entry.usage.is_some();
                    entry.error = Some(error);
                }
            }
        }
        peak
    }

    fn wake(&self) {
        self.notify.notify_one();
    }

    /// Sleeps for the interval unless something asks to go now.
    async fn idle(&self, interval: Duration) {
        let _ = tokio::time::timeout(interval, self.notify.notified()).await;
    }
}

/// Everything the app keeps in memory.
pub struct AppState {
    store: ConfigStore,
    meter: Meter,
}

impl AppState {
    /// `first_run` seeds the panel with "no key yet" instead of an empty dial.
    pub fn new(store: ConfigStore, first_run: bool) -> Self {
        // A fresh install gets one empty workspace, so the settings screen has
        // somewhere to paste a key without a second click.
        if first_run && store.workspaces().is_empty() {
            let _ = store.add_workspace(None, "", true);
        }

        let state = Self {
            store,
            meter: Meter::new(),
        };
        if first_run {
            state.meter.seed(&state.active_id(), Some(UsageError::unconfigured()));
        }
        state
    }

    /* ---------------------------------------------------------- readers */

    pub fn settings(&self) -> Settings {
        self.store.settings()
    }

    pub fn workspaces(&self) -> Vec<Workspace> {
        self.store.workspaces()
    }

    pub fn active_id(&self) -> String {
        self.store.active_id()
    }

    pub fn active_key(&self) -> String {
        self.store.active_key()
    }

    pub fn history(&self) -> Vec<Sample> {
        self.store.history()
    }

    /// The active workspace's reading.
    pub fn snapshot(&self) -> Snapshot {
        self.meter.snapshot(&self.active_id())
    }

    /// Settings + the active reading, for readers that need both (the tray).
    pub fn readout(&self) -> (Settings, Snapshot) {
        (self.store.settings(), self.snapshot())
    }


    /* --------------------------------------------------------- mutations */

    pub fn set_preferences(&self, preferences: Preferences) -> std::io::Result<Settings> {
        self.store.set_preferences(preferences)
    }

    pub fn add_workspace(&self, label: Option<String>, api_key: &str) -> std::io::Result<Settings> {
        self.store.add_workspace(label, api_key, true)
    }

    pub fn set_workspace_key(&self, id: &str, api_key: &str) -> std::io::Result<Settings> {
        self.store.set_key(id, api_key)
    }

    pub fn rename_workspace(&self, id: &str, label: &str) -> std::io::Result<Settings> {
        self.store.rename(id, label)
    }

    pub fn remove_workspace(&self, id: &str) -> std::io::Result<Settings> {
        let settings = self.store.remove(id)?;
        if let Ok(mut map) = self.meter.snapshots.lock() {
            map.remove(id);
        }
        Ok(settings)
    }

    /// Makes a workspace active and gives it a clean slate to load into.
    pub fn select_workspace(&self, id: &str) -> std::io::Result<Settings> {
        let settings = self.store.select(id)?;
        if self.store.is_configured() {
            self.meter.hold(id);
        }
        Ok(settings)
    }

    /* ------------------------------------------------------------ polling */

    /// Claims the fetch slot for a workspace and marks it as checking. False
    /// when a fetch for it is already running.
    pub fn begin_poll(&self, key_id: &str) -> bool {
        if !self.meter.claim(key_id) {
            return false;
        }
        let configured = self
            .store
            .workspaces()
            .iter()
            .any(|key| key.id == key_id && key.is_configured());
        self.meter.mark_loading(key_id, configured);
        true
    }

    /// Stores the outcome of the fetch begun by [`AppState::begin_poll`].
    pub fn finish_poll(&self, key_id: &str, result: Result<Usage, UsageError>) {
        if let Some(peak) = self.meter.settle(key_id, result) {
            self.store.record(key_id, peak);
        }
        self.meter.release(key_id);
    }

    /// Asks the poll loop to run now instead of waiting out the interval.
    pub fn request_refresh(&self) {
        self.meter.wake();
    }

    pub async fn idle(&self, interval: Duration) {
        self.meter.idle(interval).await;
    }
}

impl Meter {
    /// Marks a workspace as loading without claiming the fetch slot, so a
    /// switch shows the spinner until the poll it triggered comes back.
    fn hold(&self, key_id: &str) {
        if let Ok(mut map) = self.snapshots.lock() {
            map.entry(key_id.to_string()).or_default().loading = true;
        }
    }
}

fn poisoned(what: &str) -> std::io::Error {
    std::io::Error::other(format!("{what} lock poisoned"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::UsageWindow;

    fn temp_store(name: &str) -> ConfigStore {
        let path = std::env::temp_dir().join(format!("oc-usage-{}-{name}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        ConfigStore::open(path)
    }

    fn window(percent: f64) -> UsageWindow {
        UsageWindow {
            status: "ok".into(),
            percent,
            resets_at: "2030-01-01T00:00:00Z".into(),
        }
    }

    fn usage(rolling: f64, weekly: f64) -> Usage {
        Usage {
            rolling: window(rolling),
            weekly: window(weekly),
            monthly: window(0.0),
            balance: None,
        }
    }

    fn configured(name: &str, key: &str) -> AppState {
        let state = AppState::new(temp_store(name), false);
        state.add_workspace(None, key).expect("add");
        state
    }

    #[test]
    fn a_fresh_install_starts_with_one_empty_workspace() {
        let state = AppState::new(temp_store("fresh"), true);
        assert_eq!(state.workspaces().len(), 1, "somewhere to paste a key");
        assert!(!state.store.is_configured());
        assert_eq!(state.snapshot().error.unwrap().kind, "unconfigured");
    }

    #[test]
    fn only_one_poll_runs_at_a_time_per_workspace() {
        let state = configured("single", "oc_one_1111");
        let id = state.active_id();

        assert!(state.begin_poll(&id), "first caller gets the slot");
        assert!(!state.begin_poll(&id), "a second caller stands down");

        state.finish_poll(&id, Ok(usage(10.0, 5.0)));
        assert!(state.begin_poll(&id), "the slot is free again");
        state.finish_poll(&id, Ok(usage(11.0, 5.0)));
    }

    #[test]
    fn a_configured_failure_keeps_the_last_reading_and_flags_it_stale() {
        let state = configured("stale", "oc_one_2222");
        let id = state.active_id();

        state.begin_poll(&id);
        state.finish_poll(&id, Ok(usage(42.0, 20.0)));
        let fresh = state.snapshot();
        assert_eq!(fresh.usage.unwrap().rolling.percent, 42.0);
        assert!(!fresh.stale);
        assert!(fresh.error.is_none());
        assert!(fresh.fetched_at.is_some());

        state.begin_poll(&id);
        let recorded = state.history().len();
        assert_eq!(recorded, 1, "the successful poll recorded one sample");
        state.finish_poll(
            &id,
            Err(UsageError {
                kind: "network".into(),
                message: "No connection to opencode.ai.".into(),
                status: None,
            }),
        );

        let stale = state.snapshot();
        assert!(stale.usage.is_some(), "the numbers stay on screen");
        assert!(stale.stale, "and are marked stale");
        assert_eq!(stale.error.unwrap().kind, "network");
        assert!(!stale.loading);
        assert_eq!(state.history().len(), recorded, "a failure records no sample");
    }

    #[test]
    fn an_unconfigured_workspace_clears_usage_without_fetching() {
        let state = AppState::new(temp_store("empty"), true);
        let id = state.active_id();

        state.begin_poll(&id);
        assert!(!state.snapshot().loading, "nothing is in flight to wait for");
        state.finish_poll(&id, Err(UsageError::unconfigured()));

        let snapshot = state.snapshot();
        assert!(snapshot.usage.is_none(), "no key, nothing to show");
        assert!(!snapshot.stale);
    }

    #[test]
    fn a_successful_poll_records_a_sample_in_its_own_workspace() {
        let state = configured("history", "oc_one_3333");
        let id = state.active_id();

        state.begin_poll(&id);
        state.finish_poll(&id, Ok(usage(30.0, 44.0)));

        let history = state.history();
        assert_eq!(history.len(), 1);
        // The sample is the busiest window, not the first one.
        assert_eq!(history[0].peak, 44.0);
    }

    #[test]
    fn workspaces_keep_their_readings_and_histories_apart() {
        let state = configured("two", "oc_first_1111");
        let first = state.active_id();
        state.begin_poll(&first);
        state.finish_poll(&first, Ok(usage(80.0, 10.0)));

        state.add_workspace(Some("Work".into()), "oc_second_2222").expect("add");
        let second = state.active_id();
        assert_ne!(first, second);

        // The new active workspace has no reading yet, and the old one is not lost.
        assert!(state.snapshot().usage.is_none());
        state.begin_poll(&second);
        state.finish_poll(&second, Ok(usage(5.0, 90.0)));

        assert_eq!(state.snapshot().usage.unwrap().weekly.percent, 90.0);
        assert_eq!(state.workspaces().len(), 2);
        assert_eq!(state.history()[0].peak, 90.0, "history follows the workspace");
        assert_eq!(
            state.workspaces().iter().find(|w| w.id == first).map(|w| w.history.len()),
            Some(1),
            "the first workspace kept its own sample"
        );
    }

    #[test]
    fn selecting_a_workspace_switches_the_reading_immediately() {
        let state = configured("switch", "oc_first_4444");
        let first = state.active_id();
        state.begin_poll(&first);
        state.finish_poll(&first, Ok(usage(70.0, 7.0)));

        state.add_workspace(None, "oc_second_5555").expect("add");
        let second = state.active_id();
        state.select_workspace(&first).expect("select");

        assert_eq!(state.active_id(), first);
        assert_eq!(
            state.snapshot().usage.unwrap().rolling.percent,
            70.0,
            "the cached reading is back, without a fetch"
        );
        assert!(state.snapshot().loading, "and it is marked as refreshing");
        assert_eq!(state.snapshot().stale, false);

        // Switching to the other one is independent.
        state.select_workspace(&second).expect("select");
        assert!(state.snapshot().usage.is_none());
    }

    #[test]
    fn removing_the_active_workspace_falls_back_to_another() {
        let state = configured("remove", "oc_first_6666");
        state.add_workspace(None, "oc_second_7777").expect("add");
        let second = state.active_id();

        state.remove_workspace(&second).expect("remove");
        assert_eq!(state.workspaces().len(), 1);
        assert_ne!(state.active_id(), second, "active id is repaired");
        assert!(state.active_key().starts_with("oc_first"));
    }

    #[test]
    fn removing_the_last_workspace_leaves_nothing_active() {
        let state = configured("remove-all", "oc_only_8888");
        let only = state.active_id();
        state.remove_workspace(&only).expect("remove");

        assert!(state.workspaces().is_empty());
        assert_eq!(state.active_id(), "");
        assert!(state.snapshot().usage.is_none());
    }

    #[test]
    fn preferences_survive_and_do_not_touch_the_active_workspace() {
        let state = configured("prefs", "oc_one_9999");
        let before = state.active_id();

        state
            .set_preferences(Preferences {
                poll_seconds: 300,
                show_percent_in_menu_bar: false,
            })
            .expect("prefs");

        assert_eq!(state.settings().poll_seconds, 300);
        assert!(!state.settings().show_percent_in_menu_bar);
        assert_eq!(state.active_id(), before);
    }
}
