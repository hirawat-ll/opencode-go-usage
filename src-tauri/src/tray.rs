//! The status item: an icon, an optional percentage, and a right-click menu.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::Workspace;
use crate::panel;
use crate::state::{AppState, Snapshot};

pub const ID: &str = "meter";
const MENU_CHECK: &str = "check";
const MENU_DETAILS: &str = "details";
const MENU_SETTINGS: &str = "settings";
const MENU_QUIT: &str = "quit";

/// 44px template art, shown at 22pt so it stays crisp on Retina.
const ICON: &[u8] = include_bytes!("../icons/tray.png");

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let check = MenuItem::with_id(app, MENU_CHECK, "Check now", true, None::<&str>)?;
    let details = MenuItem::with_id(app, MENU_DETAILS, "Usage details…", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, MENU_SETTINGS, "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit OpenCode Go Usage", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&check, &details, &settings, &separator, &quit],
    )?;

    let icon = Image::from_bytes(ICON)?;

    TrayIconBuilder::with_id(ID)
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_CHECK => app.state::<AppState>().request_refresh(),
            MENU_DETAILS => {
                panel::show(app, None);
                let _ = app.emit("panel://navigate", "details");
            }
            MENU_SETTINGS => {
                panel::show(app, None);
                let _ = app.emit("panel://navigate", "settings");
            }
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                rect,
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                panel::toggle(tray.app_handle(), Some(rect));
            }
        })
        .build(app)?;

    Ok(())
}

/// Repaint the title and tooltip from the current snapshot.
pub fn update(app: &AppHandle) {
    let (title, tooltip) = current(app);
    let handle = app.clone();
    // AppKit work has to happen on the main thread.
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id(ID) else {
            return;
        };
        let _ = tray.set_title(if title.is_empty() { None } else { Some(title) });
        let _ = tray.set_tooltip(Some(tooltip));
    });
}

/// Gathers what [`describe`] needs from the app.
fn current(app: &AppHandle) -> (String, String) {
    let state = app.state::<AppState>();
    let (settings, snapshot) = state.readout();
    describe(
        &state.workspaces(),
        &state.active_id(),
        settings.show_percent_in_menu_bar,
        &snapshot,
    )
}

/// The menu bar readout for one workspace. Pure, so the wording is covered by
/// tests rather than by squinting at the menu bar.
pub(crate) fn describe(
    workspaces: &[Workspace],
    active_id: &str,
    show_percent: bool,
    snapshot: &Snapshot,
) -> (String, String) {
    let active = workspaces.iter().find(|workspace| workspace.id == active_id);

    // Only name the workspace when there is more than one to tell apart.
    let prefix = match (workspaces.len(), active) {
        (count, Some(workspace)) if count > 1 && !workspace.label.is_empty() => {
            format!("{} · ", workspace.label)
        }
        _ => String::new(),
    };

    let title = match (&snapshot.usage, show_percent) {
        (Some(usage), true) => format!(" {}%", usage.peak().round() as i64),
        _ => String::new(),
    };

    let tooltip = match &snapshot.usage {
        Some(usage) => format!(
            "OpenCode Go — {prefix}5-hour {}% · week {}% · month {}%",
            usage.rolling.percent.round() as i64,
            usage.weekly.percent.round() as i64,
            usage.monthly.percent.round() as i64,
        ),
        None => match snapshot.error.as_ref().map(|e| e.kind.as_str()) {
            Some("unauthorized") => format!("OpenCode Go — {prefix}the API key was rejected"),
            Some("no_subscription") => format!("OpenCode Go — {prefix}no Go plan on this workspace"),
            Some("network") | Some("http") => format!("OpenCode Go — {prefix}cannot reach opencode.ai"),
            Some("unconfigured") => format!("OpenCode Go — {prefix}add your API key"),
            _ => "OpenCode Go".to_string(),
        },
    };

    (title, tooltip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{Usage, UsageError, UsageWindow};

    fn usage(rolling: f64, weekly: f64, monthly: f64) -> Usage {
        let window = |percent: f64| UsageWindow {
            status: "ok".into(),
            percent,
            resets_at: "2030-01-01T00:00:00Z".into(),
        };
        Usage {
            rolling: window(rolling),
            weekly: window(weekly),
            monthly: window(monthly),
            balance: None,
        }
    }

    fn workspace(id: &str, label: &str) -> Workspace {
        Workspace {
            id: id.into(),
            label: label.into(),
            api_key: "oc_test_key".into(),
            history: Vec::new(),
        }
    }

    fn with_usage(usage: Option<Usage>) -> Snapshot {
        Snapshot {
            usage,
            ..Snapshot::default()
        }
    }

    #[test]
    fn a_single_workspace_is_never_named() {
        let (title, tooltip) = describe(
            &[workspace("a", "Personal")],
            "a",
            true,
            &with_usage(Some(usage(47.2, 31.0, 6.4))),
        );
        assert_eq!(title, " 47%");
        assert_eq!(
            tooltip,
            "OpenCode Go — 5-hour 47% · week 31% · month 6%",
            "no label when there is only one workspace"
        );
    }

    #[test]
    fn several_workspaces_name_the_active_one() {
        let workspaces = [workspace("a", "Personal"), workspace("b", "Work")];
        let (_, tooltip) = describe(&workspaces, "b", true, &with_usage(Some(usage(1.4, 2.6, 3.1))));
        assert!(
            tooltip.starts_with("OpenCode Go — Work · 5-hour 1%"),
            "expected the active label: {tooltip}"
        );
    }

    #[test]
    fn the_title_uses_the_busiest_window_and_can_be_switched_off() {
        let mut snapshot = with_usage(Some(usage(12.0, 88.0, 40.0)));
        let workspaces = [workspace("a", "Personal")];
        assert_eq!(describe(&workspaces, "a", true, &snapshot).0, " 88%");

        snapshot = with_usage(Some(usage(12.0, 88.0, 40.0)));
        assert_eq!(
            describe(&workspaces, "a", false, &snapshot).0,
            "",
            "the preference removes the number"
        );
    }

    #[test]
    fn a_failure_has_no_number_but_says_what_happened() {
        let rejected = Snapshot {
            error: Some(UsageError {
                kind: "unauthorized".into(),
                message: "Unauthorized".into(),
                status: Some(401),
            }),
            ..Snapshot::default()
        };
        let (title, tooltip) = describe(&[workspace("a", "Personal")], "a", true, &rejected);
        assert_eq!(title, "");
        assert_eq!(tooltip, "OpenCode Go — the API key was rejected");
    }

    #[test]
    fn a_workspace_without_a_key_is_described_as_unconfigured() {
        let unconfigured = Snapshot {
            error: Some(UsageError::unconfigured()),
            ..Snapshot::default()
        };
        let (title, tooltip) = describe(&[workspace("a", "Personal")], "a", true, &unconfigured);
        assert_eq!(title, "");
        assert!(tooltip.contains("add your API key"), "{tooltip}");
    }
}
