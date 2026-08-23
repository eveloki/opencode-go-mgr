pub mod autostart;
pub mod host;
pub mod native_browser;
pub mod state;
pub mod tray;
pub mod updater;

pub type Result<T> = anyhow::Result<T>;

use ocg_core::crypto::KeyCipher;
#[cfg(windows)]
use ocg_core::crypto::MachineBoundCipher;
#[cfg(not(windows))]
use ocg_core::crypto::load_or_create_static_cipher;
use ocg_core::db::Database;
use ocg_core::state::CoreStateInner;
use parking_lot::Mutex;
use state::{BrowserProcessState, GuiState};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

pub fn run() {
    let data_dir = data_dir();
    let cipher = match load_cipher(&data_dir) {
        Ok(cipher) => cipher,
        Err(e) => {
            eprintln!("failed to initialize encryption: {}", e);
            std::process::exit(1);
        }
    };
    let db = match Database::open(data_dir.clone()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("failed to open database: {}", e);
            std::process::exit(1);
        }
    };

    let core_state = match CoreStateInner::new(db, data_dir.clone(), cipher) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("failed to initialize state: {}", e);
            std::process::exit(1);
        }
    };

    host::register_desktop_settings(&core_state);

    let browser_processes = Arc::new(Mutex::new(BrowserProcessState::default()));
    host::register_native_browser(&core_state, browser_processes.clone());
    host::gateway::start_on_configured_port(&core_state);

    let gui_state = Arc::new(GuiState {
        core: core_state.clone(),
        browser_processes,
    });

    let app_state = gui_state.clone();
    let setup_core_state = core_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !args.iter().any(|arg| arg == "--startup") {
                tray::open_dashboard(app);
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .manage(app_state.clone())
        .setup(move |app| {
            if let Ok(resource_dir) = app.path().resource_dir() {
                setup_core_state.set_dashboard_dir(Some(resource_dir.join("dist")));
            }
            host::register_dock_visibility(&setup_core_state, app);
            updater::configure(app.handle(), setup_core_state.clone())?;
            tray::setup_tray(app)?;
            if !autostart::is_startup_launch() {
                tray::open_dashboard(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().ok();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |_app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                host::close_native_browsers(&app_state.browser_processes, &core_state.data_dir());
                host::gateway::stop_listener(&core_state);
                let _ = core_state
                    .db
                    .lock()
                    .log_gateway("info", "gateway", "application exiting");
            }
        });
}

fn load_cipher(data_dir: &std::path::Path) -> Result<Arc<dyn KeyCipher + Send + Sync>> {
    #[cfg(windows)]
    {
        let _ = data_dir;
        Ok(Arc::new(MachineBoundCipher::new()))
    }
    #[cfg(not(windows))]
    {
        Ok(Arc::new(load_or_create_static_cipher(data_dir)?))
    }
}

fn data_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    home.join(".ocg-mgr")
}

#[cfg(test)]
mod host_lifecycle_surface {
    #[test]
    fn desktop_capabilities_and_exit_are_separate_from_gateway_and_updater() {
        // Source-text: live tray/AppHandle construction is not available in
        // crate unit tests without a packaged WebView runtime.
        let production = include_str!("lib.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production lib.rs precedes this test module");
        assert!(production.contains("home.join(\".ocg-mgr\")"));
        assert!(production.contains("host::gateway::stop_listener"));
        assert!(
            !production.contains("usage_sync"),
            "application exit stops the gateway listener and does not cancel the CoreState usage worker"
        );
        assert!(production.contains("host::register_desktop_settings"));
        assert!(production.contains("host::register_dock_visibility"));
        assert!(production.contains("updater::configure"));

        let capabilities = include_str!("../capabilities/default.json");
        assert!(capabilities.contains("core:window:allow-hide"));
        assert!(!capabilities.contains("updater"));
    }

    #[test]
    fn native_browser_hooks_are_owned_by_the_host_module() {
        let production = include_str!("lib.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production lib.rs precedes this test module");
        assert!(production.contains("host::register_native_browser"));
        assert!(production.contains("host::close_native_browsers"));
        assert!(!production.contains("pub mod commands"));
        assert!(!production.contains("commands::"));
    }

    #[test]
    fn no_tauri_invoke_commands_remain() {
        let production = include_str!("lib.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production lib.rs precedes this test module");
        assert!(
            !production.contains("invoke_handler"),
            "WebView invoke commands must be removed"
        );
        assert!(
            !production.contains("tauri::generate_handler"),
            "no generate_handler registrations may remain"
        );
        assert!(!production.contains("#[tauri::command]"));

        for path in [
            include_str!("host/mod.rs"),
            include_str!("host/gateway.rs"),
            include_str!("native_browser.rs"),
            include_str!("updater.rs"),
            include_str!("tray.rs"),
            include_str!("autostart.rs"),
            include_str!("state.rs"),
        ] {
            let source = path.split("#[cfg(test)]").next().unwrap_or(path);
            assert!(
                !source.contains("#[tauri::command]"),
                "host capability modules must not register WebView commands"
            );
            assert!(!source.contains("tauri::generate_handler"));
        }
    }
}
