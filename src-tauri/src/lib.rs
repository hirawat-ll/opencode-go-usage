//! OpenCode Go Usage — a menu bar meter for OpenCode Go plan limits.
//!
//! Wiring only: the loop that polls, the commands the panel calls, and the
//! window/tray setup. The pieces live next door — [`state`] for what is known,
//! [`usage`] for the endpoint, [`localdb`] for token detail, [`panel`] and
//! [`tray`] for the two surfaces, [`ipc`] for the wire shapes.

mod config;
mod ipc;
mod localdb;
mod login_item;
mod panel;
mod state;
mod tray;
mod usage;

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

use config::Preferences;
use ipc::{panel_state, PanelState, TestResult};
use state::{AppState, ConfigStore};

/* --------------------------------------------------------------- helpers */

fn broadcast(app: &AppHandle) {
    let _ = app.emit("usage://update", panel_state(app));
}

/* --------------------------------------------------------------- polling */

async fn poll_loop(app: AppHandle) {
    loop {
        refresh_once(&app).await;

        let state = app.state::<AppState>();
        state.idle(Duration::from_secs(state.settings().poll_seconds.max(15))).await;
    }
}

/// One poll of the active workspace. Inactive workspaces are refreshed when
/// they are selected, which keeps the request rate at one workspace at a time.
async fn refresh_once(app: &AppHandle) {
    let state = app.state::<AppState>();
    let key_id = state.active_id();
    if key_id.is_empty() || !state.begin_poll(&key_id) {
        return;
    }
    // Let the panel show "Checking…" before the request goes out.
    broadcast(app);

    let result = usage::fetch(&state.active_key()).await;
    state.finish_poll(&key_id, result);

    tray::update(app);
    broadcast(app);
}

/* -------------------------------------------------------------- commands */

#[tauri::command]
fn get_state(app: AppHandle) -> PanelState {
    panel_state(&app)
}

/// The two preferences the panel owns; the workspace list has its own commands.
#[tauri::command]
fn save_preferences(app: AppHandle, preferences: Preferences) -> Result<PanelState, String> {
    let state = app.state::<AppState>();
    state
        .set_preferences(preferences)
        .map_err(|e| e.to_string())?;
    tray::update(&app);
    broadcast(&app);
    Ok(panel_state(&app))
}

#[tauri::command]
fn add_workspace(app: AppHandle, label: Option<String>, api_key: String) -> Result<PanelState, String> {
    let state = app.state::<AppState>();
    state
        .add_workspace(label, &api_key)
        .map_err(|e| e.to_string())?;
    tray::update(&app);
    broadcast(&app);
    state.request_refresh();
    Ok(panel_state(&app))
}

#[tauri::command]
fn set_workspace_key(app: AppHandle, id: String, api_key: String) -> Result<PanelState, String> {
    let state = app.state::<AppState>();
    state
        .set_workspace_key(&id, &api_key)
        .map_err(|e| e.to_string())?;
    broadcast(&app);
    if state.active_id() == id {
        state.request_refresh();
    }
    Ok(panel_state(&app))
}

#[tauri::command]
fn rename_workspace(app: AppHandle, id: String, label: String) -> Result<PanelState, String> {
    let state = app.state::<AppState>();
    state.rename_workspace(&id, &label).map_err(|e| e.to_string())?;
    tray::update(&app);
    broadcast(&app);
    Ok(panel_state(&app))
}

#[tauri::command]
fn select_workspace(app: AppHandle, id: String) -> Result<PanelState, String> {
    let state = app.state::<AppState>();
    state.select_workspace(&id).map_err(|e| e.to_string())?;
    tray::update(&app);
    broadcast(&app);
    // The newly active workspace is the one worth polling now.
    state.request_refresh();
    Ok(panel_state(&app))
}

#[tauri::command]
fn remove_workspace(app: AppHandle, id: String) -> Result<PanelState, String> {
    let state = app.state::<AppState>();
    let was_active = state.active_id() == id;
    state.remove_workspace(&id).map_err(|e| e.to_string())?;
    if was_active {
        state.request_refresh();
    }
    tray::update(&app);
    broadcast(&app);
    Ok(panel_state(&app))
}

#[tauri::command]
async fn refresh(app: AppHandle) -> Result<PanelState, String> {
    refresh_once(&app).await;
    Ok(panel_state(&app))
}

/// Checks a key the user has typed, without touching the stored one.
#[tauri::command]
async fn test_key(api_key: String) -> TestResult {
    let key = api_key.trim();
    if key.is_empty() {
        return TestResult::rejected("Enter a key first.");
    }
    match usage::fetch(key).await {
        Ok(_) => TestResult::ok(),
        Err(error) => TestResult::failed(&error),
    }
}

/// Token detail lives in OpenCode's own database, so the read runs on a worker
/// thread instead of the IPC thread. `go_only` narrows it to the providers the
/// Go plan serves; the filtering happens in SQL so the chart and the totals
/// cannot disagree about which requests they are counting.
#[tauri::command]
async fn usage_detail(range: String, go_only: bool) -> localdb::Detail {
    tauri::async_runtime::spawn_blocking(move || localdb::detail(&range, go_only))
        .await
        .unwrap_or_else(|_| {
            localdb::Detail::unavailable("24h", go_only, "The read was cancelled.")
        })
}

#[tauri::command]
fn resize_panel(app: AppHandle, height: f64) {
    panel::resize(&app, height);
}

#[tauri::command]
fn hide_panel(app: AppHandle) {
    panel::hide(&app);
}

/// Whether macOS will start the app at login.
#[tauri::command]
async fn login_item_status(app: AppHandle) -> Result<String, String> {
    on_main(&app, || login_item::status().as_str().to_string())
        .await
        .ok_or_else(|| "The main thread did not answer.".to_string())
}

/// Turns the login item on or off, and reports what macOS decided.
#[tauri::command]
async fn set_login_item(app: AppHandle, enabled: bool) -> Result<String, String> {
    on_main(&app, move || {
        login_item::set_enabled(enabled).map(|state| state.as_str().to_string())
    })
    .await
    .ok_or_else(|| "The main thread did not answer.".to_string())?
}

/// ServiceManagement is an AppKit-adjacent framework, so its calls go to the
/// main thread. The result comes back over a channel to keep the command
/// synchronous for the caller.
async fn on_main<T, F>(app: &AppHandle, work: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(work());
    })
    .ok()?;
    receiver.recv().ok()
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/* ------------------------------------------------------------------- run */

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_preferences,
            add_workspace,
            set_workspace_key,
            rename_workspace,
            select_workspace,
            remove_workspace,
            refresh,
            test_key,
            usage_detail,
            login_item_status,
            set_login_item,
            resize_panel,
            hide_panel,
            quit
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let path = config::config_path(app.path().app_config_dir()?);
            let store = ConfigStore::open(path);
            let first_run = !store.is_configured();
            app.manage(AppState::new(store, first_run));

            // An item left behind by the LaunchAgent era shows up in System
            // Settings as an anonymous background agent; move it over to
            // ServiceManagement so it carries the app's name and icon.
            login_item::migrate_legacy();

            #[cfg(target_os = "macos")]
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            tray::build(&handle)?;
            install_panel_behaviour(&handle);
            tray::update(&handle);

            tauri::async_runtime::spawn(poll_loop(handle.clone()));

            if first_run {
                // Nothing to meter yet: open the panel on the settings view.
                panel::show(&handle, None);
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build OpenCode Go Usage")
        .run(|_app, event| {
            // Closing the panel must not take the meter down with it, but an
            // explicit quit still has to work.
            if let RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

/// A menu bar panel closes when it loses focus, and stays alive when its
/// window is "closed".
fn install_panel_behaviour(app: &AppHandle) {
    let Some(window) = panel::window(app) else {
        return;
    };
    let panel_window = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Focused(false) => {
            if panel_window.is_visible().unwrap_or(false) {
                let _ = panel_window.hide();
                panel::note_auto_hide();
            }
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = panel_window.hide();
        }
        _ => {}
    });
}
