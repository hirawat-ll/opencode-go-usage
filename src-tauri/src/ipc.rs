//! The shapes that cross the IPC boundary.
//!
//! They live apart from [`crate::state`] so the runtime state can be
//! reorganised without changing what the panel sees, and so the wire format is
//! described in exactly one place per object. API keys never cross this
//! boundary — the panel gets a masked tail it can display, and nothing more.

use serde::Serialize;
use tauri::AppHandle;
use tauri::Manager;

use crate::config::{Preferences, Sample, Workspace};
use crate::localdb::LocalSource;
use crate::state::AppState;
use crate::usage::{Usage, UsageError};

/// One workspace, as the panel sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub id: String,
    pub label: String,
    pub active: bool,
    /// False when no key is stored yet.
    pub configured: bool,
    /// Masked tail of the key, e.g. "…a91f".
    pub preview: String,
}

impl WorkspaceInfo {
    pub fn of(workspace: &Workspace, active_id: &str) -> Self {
        Self {
            id: workspace.id.clone(),
            label: workspace.label.clone(),
            active: workspace.id == active_id,
            configured: workspace.is_configured(),
            preview: Workspace::key_tail(&workspace.api_key),
        }
    }
}

/// Everything the panel needs in one object.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelState {
    pub settings: Preferences,
    pub workspaces: Vec<WorkspaceInfo>,
    pub active_key_id: String,
    pub usage: Option<Usage>,
    pub error: Option<UsageError>,
    pub fetched_at: Option<i64>,
    pub stale: bool,
    pub loading: bool,
    pub history: Vec<Sample>,
    pub version: String,
    /// Token detail is a separate, local source; the panel says so instead of
    /// implying it needs the API key.
    pub local: LocalSource,
}

/// The answer to "is this key any good?".
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    pub kind: String,
    pub message: String,
}

impl TestResult {
    pub fn ok() -> Self {
        Self {
            ok: true,
            kind: "ok".into(),
            message: "Connected — Go plan active.".into(),
        }
    }

    pub fn rejected(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            kind: "unauthorized".into(),
            message: message.into(),
        }
    }

    pub fn failed(error: &UsageError) -> Self {
        Self {
            ok: false,
            kind: error.kind.clone(),
            message: crate::usage::explain(error),
        }
    }
}

pub fn panel_state(app: &AppHandle) -> PanelState {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let active_id = state.active_id();
    let snapshot = state.snapshot();

    PanelState {
        settings: Preferences::from(&settings),
        workspaces: state
            .workspaces()
            .iter()
            .map(|workspace| WorkspaceInfo::of(workspace, &active_id))
            .collect(),
        active_key_id: active_id,
        usage: snapshot.usage,
        error: snapshot.error,
        fetched_at: snapshot.fetched_at,
        stale: snapshot.stale,
        loading: snapshot.loading,
        history: state.history(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        local: crate::localdb::source(),
    }
}
