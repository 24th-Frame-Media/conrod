#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use conrod_engine::commands::Command;
use conrod_engine::desktop::Desktop;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::Manager;
use tauri::{PhysicalPosition, PhysicalSize};

#[tauri::command]
async fn command(
    desktop: tauri::State<'_, Arc<Desktop>>,
    action: String,
    args: Value,
) -> Result<Value, String> {
    let desktop = desktop.inner().clone();
    tauri::async_runtime::spawn_blocking(move || desktop.dispatch(&action, args))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn choose_folder() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Choose a photo folder")
            .pick_folder()
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())
}

/// The window's last normal (not maximised, not minimised) placement.
#[derive(Clone, Copy)]
struct Bounds {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    maximized: bool,
}
struct Remembered(Mutex<Option<Bounds>>);

fn bounds_file() -> std::path::PathBuf {
    conrod_core::settings::data_root().join("window.json")
}

fn load_bounds() -> Option<Bounds> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(bounds_file()).ok()?).ok()?;
    Some(Bounds {
        x: v["x"].as_i64()? as i32,
        y: v["y"].as_i64()? as i32,
        w: v["w"].as_u64()? as u32,
        h: v["h"].as_u64()? as u32,
        maximized: v["maximized"].as_bool().unwrap_or(false),
    })
}

fn save_bounds(b: Bounds) {
    let json =
        serde_json::json!({"x": b.x, "y": b.y, "w": b.w, "h": b.h, "maximized": b.maximized});
    let _ = std::fs::write(bounds_file(), json.to_string());
}

/// Put the window back where it was, unless that place is no longer on a screen.
fn restore(window: &tauri::WebviewWindow, b: Bounds) {
    let on_screen = window.available_monitors().is_ok_and(|monitors| {
        monitors.iter().any(|m| {
            let (p, s) = (m.position(), m.size());
            b.x + 80 >= p.x
                && b.y + 40 >= p.y
                && b.x + 80 < p.x + s.width as i32
                && b.y + 40 < p.y + s.height as i32
        })
    });
    let _ = window.set_size(PhysicalSize::new(b.w, b.h));
    if on_screen {
        let _ = window.set_position(PhysicalPosition::new(b.x, b.y));
    } else {
        let _ = window.center();
    }
    if b.maximized {
        let _ = window.maximize();
    }
}

/// Remember the placement while the window is in its normal state.
fn remember(window: &tauri::Window) {
    if window.is_maximized().unwrap_or(false) || window.is_minimized().unwrap_or(false) {
        return;
    }
    if let (Ok(p), Ok(s)) = (window.outer_position(), window.outer_size()) {
        *window.state::<Remembered>().0.lock().unwrap() = Some(Bounds {
            x: p.x,
            y: p.y,
            w: s.width,
            h: s.height,
            maximized: false,
        });
    }
}

fn save_placement(window: &tauri::Window) {
    remember(window);
    let last = *window.state::<Remembered>().0.lock().unwrap();
    if let Some(mut b) = last {
        b.maximized = window.is_maximized().unwrap_or(false);
        save_bounds(b);
    }
}

/// What the taskbar button should show: the scan, identify or write in flight.
fn taskbar(desktop: &Desktop) -> ProgressBarState {
    use conrod_core::tasks::State;
    let tasks = desktop.hub.snapshot();
    let active = tasks.iter().find(|t| {
        t.total > 0
            && matches!(t.state, State::Running | State::Paused)
            && ["Culling", "Identifying", "Writing"]
                .iter()
                .any(|p| t.label.starts_with(p))
    });
    match active {
        Some(t) => ProgressBarState {
            status: Some(if t.state == State::Paused {
                ProgressBarStatus::Paused
            } else {
                ProgressBarStatus::Normal
            }),
            progress: Some((t.done * 100 / t.total).min(100)),
        },
        None => ProgressBarState {
            status: Some(ProgressBarStatus::None),
            progress: None,
        },
    }
}

/// `Conrod.exe --selftest [report.txt]`: the release's own proof that it can work.
/// The GUI build has no console, so the report goes to a file (default: the temp folder).
fn selftest() -> ! {
    let (code, report) = conrod_engine::selftest::run();
    let path = std::env::args()
        .skip_while(|a| a != "--selftest")
        .nth(1)
        .map_or_else(
            || std::env::temp_dir().join("conrod-selftest.txt"),
            Into::into,
        );
    let _ = std::fs::write(path, report);
    std::process::exit(code);
}

fn main() {
    if std::env::args().any(|a| a == "--selftest") {
        selftest();
    }
    // One window per data directory: a run with its own CONROD_HOME (an isolated
    // or portable library, a test) is a separate instance.
    let mut builder = tauri::Builder::default();
    if std::env::var_os("CONROD_HOME").is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }
    let builder = builder
        .setup(|app| {
            let desktop =
                Desktop::open(conrod_core::settings::data_root()).map_err(std::io::Error::other)?;
            app.asset_protocol_scope()
                .allow_directory(desktop.root.join("cache"), true)?;
            app.manage(desktop);
            app.manage(Remembered(Mutex::new(None)));
            if let (Some(window), Some(bounds)) = (app.get_webview_window("main"), load_bounds()) {
                restore(&window, bounds);
            }
            let (handle, worker) = (
                app.handle().clone(),
                app.state::<Arc<Desktop>>().inner().clone(),
            );
            // Push the status to the window when (and only when) a task changes, and
            // mirror it onto the taskbar button. The frontend used to poll for this.
            std::thread::spawn(move || {
                use tauri::Emitter;
                let mut seen = u64::MAX;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    if worker.quit_requested() {
                        handle.exit(0);
                        return;
                    }
                    let version = worker.hub.version();
                    if version == seen {
                        continue;
                    }
                    seen = version;
                    let _ = handle.emit("status", worker.status());
                    if let Some(window) = handle.get_webview_window("main") {
                        let _ = window.set_progress_bar(taskbar(&worker));
                    }
                }
            });
            let show =
                tauri::menu::MenuItem::with_id(app, "show", "Open Conrod", true, None::<&str>)?;
            let quit = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show, &quit])?;
            let mut tray = tauri::tray::TrayIconBuilder::new()
                .tooltip("Conrod")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "quit" {
                        let _ = app.state::<Arc<Desktop>>().run(Command::Stop {});
                        app.exit(0);
                    } else if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(
                event,
                tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Moved(_)
            ) {
                remember(window);
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                save_placement(window);
                let desktop = window.state::<Arc<Desktop>>();
                let close_to_tray = desktop.close_to_tray();
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    let _ = desktop.run(Command::Stop {});
                }
            }
        })
        .invoke_handler(tauri::generate_handler![command, choose_folder]);
    if let Err(error) = builder.run(tauri::generate_context!()) {
        rfd::MessageDialog::new()
            .set_title("Conrod could not start")
            .set_description(error.to_string())
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}
