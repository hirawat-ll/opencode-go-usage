//! The login item.
//!
//! macOS 13 and newer have `SMAppService`, which registers the app *as itself*:
//! System Settings → Login Items then lists the app's name and icon, and the
//! record points at the app bundle. The older `LaunchAgent` route (a plist in
//! `~/Library/LaunchAgents`) still starts the app, but macOS files it as an
//! anonymous background agent — "Unknown Developer", no icon — which is why
//! this module exists at all. Older systems get the plist as a fallback.
//!
//! Calls into `ServiceManagement` are dispatched to the main thread; the class
//! is looked up at runtime so the binary still runs on macOS 12.

use std::path::PathBuf;

/// What the app named its launch agent before `SMAppService`; kept so an
/// enabled item can be migrated rather than dropped.
const LEGACY_LABEL: &str = "OpenCode Go Usage";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginItem {
    Enabled,
    Disabled,
    /// macOS is waiting for the user to allow it in System Settings.
    RequiresApproval,
    /// macOS 12 or older: no ServiceManagement, and no fallback wanted.
    Unsupported,
}

impl LoginItem {
    pub fn as_str(self) -> &'static str {
        match self {
            LoginItem::Enabled => "enabled",
            LoginItem::Disabled => "disabled",
            LoginItem::RequiresApproval => "requires_approval",
            LoginItem::Unsupported => "unsupported",
        }
    }
}

/// `~/Library/LaunchAgents/OpenCode Go Usage.plist`
fn legacy_plist() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library/LaunchAgents")
            .join(format!("{LEGACY_LABEL}.plist")),
    )
}

/// Registers the app with macOS. Returns the state as macOS reports it, which
/// may be `RequiresApproval` even though the call succeeded.
pub fn set_enabled(enabled: bool) -> Result<LoginItem, String> {
    #[cfg(target_os = "macos")]
    if service::available() {
        let outcome = if enabled {
            service::register()
        } else {
            service::unregister()
        };
        outcome?;
        return Ok(status());
    }

    // Older macOS: the plist is the whole mechanism.
    legacy_set(enabled)?;
    Ok(status())
}

pub fn status() -> LoginItem {
    #[cfg(target_os = "macos")]
    if service::available() {
        return match service::status() {
            Some(service::STATUS_ENABLED) => LoginItem::Enabled,
            Some(service::STATUS_REQUIRES_APPROVAL) => LoginItem::RequiresApproval,
            _ => LoginItem::Disabled,
        };
    }

    match legacy_plist() {
        // No home directory to write a launch agent into: nothing can be done.
        None => LoginItem::Unsupported,
        Some(path) if path.exists() => LoginItem::Enabled,
        Some(_) => LoginItem::Disabled,
    }
}

/// Moves an item enabled with the old LaunchAgent mechanism over to
/// `SMAppService`, so an upgrade fixes the entry rather than leaving a second,
/// anonymous one behind. Does nothing when the old item was never enabled, or
/// when this macOS cannot do better.
pub fn migrate_legacy() {
    #[cfg(target_os = "macos")]
    {
        if !service::available() {
            return;
        }
        let Some(path) = legacy_plist() else { return };
        if !path.exists() {
            return;
        }
        // The plist existing *is* the old preference: the user had it on.
        if std::fs::remove_file(&path).is_err() {
            return;
        }
        let _ = service::register();
    }
}

fn legacy_set(enabled: bool) -> Result<(), String> {
    let Some(path) = legacy_plist() else {
        return Err("Could not locate the LaunchAgents folder.".into());
    };
    if !enabled {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
        return Ok(());
    }

    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let app_path = app_bundle_path(&executable)?;
    let parent = path.parent().ok_or("Bad LaunchAgents path")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LEGACY_LABEL}</string>
    <key>ProgramArguments</key>
    <array><string>{}</string></array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
        app_path.display()
    );
    std::fs::write(&path, plist).map_err(|error| error.to_string())
}

/// `…/OpenCode Go Usage.app/Contents/MacOS/opencode-go-usage` → `…/OpenCode Go Usage.app`
fn app_bundle_path(executable: &std::path::Path) -> Result<PathBuf, String> {
    executable
        .ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        .map(|path| path.to_path_buf())
        .ok_or_else(|| "This copy of the app is not inside an app bundle.".to_string())
}

#[cfg(target_os = "macos")]
mod service {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool};

    // The framework has to be loaded for the class to exist at runtime.
    #[link(name = "ServiceManagement", kind = "framework")]
    extern "C" {}

    pub const STATUS_ENABLED: isize = 1;
    pub const STATUS_REQUIRES_APPROVAL: isize = 2;

    pub fn available() -> bool {
        AnyClass::get(c"SMAppService").is_some()
    }

    /// The service object for the running app's bundle.
    fn main_app() -> Option<*mut AnyObject> {
        let class = AnyClass::get(c"SMAppService")?;
        Some(unsafe { msg_send![class, mainAppService] })
    }

    pub fn status() -> Option<isize> {
        let service = main_app()?;
        Some(unsafe { msg_send![service, status] })
    }

    pub fn register() -> Result<(), String> {
        let service = main_app().ok_or("ServiceManagement is unavailable")?;
        let mut error: *mut AnyObject = std::ptr::null_mut();
        let ok: Bool = unsafe { msg_send![service, registerAndReturnError: &mut error] };
        if ok.as_bool() {
            Ok(())
        } else {
            Err(failure("Could not register the login item", error, service))
        }
    }

    pub fn unregister() -> Result<(), String> {
        let service = main_app().ok_or("ServiceManagement is unavailable")?;
        let mut error: *mut AnyObject = std::ptr::null_mut();
        let ok: Bool = unsafe { msg_send![service, unregisterAndReturnError: &mut error] };
        if ok.as_bool() {
            Ok(())
        } else {
            Err(failure("Could not remove the login item", error, service))
        }
    }

    /// The error's text is not worth a Foundation binding; the status is what
    /// the caller can act on.
    fn failure(what: &str, _error: *mut AnyObject, service: *mut AnyObject) -> String {
        let status: isize = unsafe { msg_send![service, status] };
        format!("{what} (macOS status {status})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundle_is_found_from_the_executable() {
        let path = app_bundle_path(std::path::Path::new(
            "/Applications/OpenCode Go Usage.app/Contents/MacOS/opencode-go-usage",
        ))
        .expect("bundle");
        assert_eq!(path.to_string_lossy(), "/Applications/OpenCode Go Usage.app");
    }

    #[test]
    fn a_bare_binary_is_reported_rather_than_guessed() {
        let error = app_bundle_path(std::path::Path::new("/usr/local/bin/thing")).unwrap_err();
        assert!(error.contains("not inside an app bundle"), "{error}");
    }

    #[test]
    fn the_legacy_plist_lives_where_the_old_versions_put_it() {
        let path = legacy_plist().expect("home");
        assert!(path.ends_with("Library/LaunchAgents/OpenCode Go Usage.plist"));
    }

    #[test]
    fn states_have_stable_names_for_the_panel() {
        assert_eq!(LoginItem::Enabled.as_str(), "enabled");
        assert_eq!(LoginItem::RequiresApproval.as_str(), "requires_approval");
        assert_eq!(LoginItem::Unsupported.as_str(), "unsupported");
    }
}
