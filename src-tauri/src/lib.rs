mod audio;
mod controller;
mod hotkeys;
mod i18n;
mod insert;
mod log;
mod overlay;
mod settings;
mod soniox;
mod tray;

use std::sync::Arc;
use tauri::{AppHandle, Manager, WindowEvent};

use controller::{Controller, Snapshot};
use hotkeys::{Action, HotkeyMsg, Shortcut};
use settings::{InsertMode, Settings};

pub(crate) fn ctl(app: &AppHandle) -> Arc<Controller> {
    app.state::<Arc<Controller>>().inner().clone()
}

pub(crate) fn hotkeys_reset() {
    #[cfg(windows)]
    hotkeys::reset_mode();
}

pub(crate) fn hotkeys_update(push_to_talk: Shortcut, hands_free: Shortcut) {
    #[cfg(windows)]
    hotkeys::update(push_to_talk, hands_free);
    #[cfg(not(windows))]
    let _ = (push_to_talk, hands_free);
}

pub(crate) fn insert_text(text: &str, mode: InsertMode) -> Result<(), String> {
    #[cfg(windows)]
    {
        match mode {
            InsertMode::Paste => insert::paste(text),
            InsertMode::Type => insert::type_text(text),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (text, mode);
        Err("text insertion is only implemented on Windows".into())
    }
}

#[tauri::command]
fn get_state(app: AppHandle) -> Snapshot {
    ctl(&app).snapshot()
}

#[tauri::command]
fn start_dictation(app: AppHandle) {
    ctl(&app).start();
}

#[tauri::command]
fn stop_dictation(app: AppHandle) {
    ctl(&app).stop();
}

#[tauri::command]
fn toggle_dictation(app: AppHandle) {
    ctl(&app).toggle();
}

#[tauri::command]
fn clear_error(app: AppHandle) {
    ctl(&app).clear_error();
}

#[tauri::command]
fn update_settings(app: AppHandle, settings: Settings) {
    ctl(&app).update_settings(settings);
}

#[tauri::command]
fn get_api_key() -> String {
    settings::load_api_key()
}

#[tauri::command]
fn save_api_key(app: AppHandle, key: String) -> Result<(), String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        settings::remove_api_key()?;
    } else {
        settings::save_api_key(&key)?;
    }
    ctl(&app).set_has_api_key(!key.is_empty());
    Ok(())
}

#[tauri::command]
fn remove_api_key(app: AppHandle) -> Result<(), String> {
    settings::remove_api_key()?;
    ctl(&app).set_has_api_key(false);
    Ok(())
}

#[tauri::command]
fn list_microphones() -> Vec<String> {
    audio::input_device_names()
}

/// `target` is "pushToTalk" or "handsFree"; the next key press is captured by the hook.
#[tauri::command]
fn record_shortcut(app: AppHandle, target: Option<String>) {
    #[cfg(windows)]
    hotkeys::set_recording(target.is_some());
    ctl(&app).set_recording_shortcut(target);
}

#[tauri::command]
fn reset_shortcuts(app: AppHandle) {
    let controller = ctl(&app);
    let mut settings = controller.settings();
    settings.push_to_talk = Shortcut::default_push_to_talk();
    settings.hands_free = Shortcut::default_hands_free();
    controller.update_settings(settings);
}

#[tauri::command]
fn open_microphone_privacy() {
    let _ = std::process::Command::new("explorer.exe").arg("ms-settings:privacy-microphone").spawn();
}

#[tauri::command]
fn open_log_folder() {
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        let dir = std::path::PathBuf::from(base).join("Kotev").join("logs");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
    }
}

fn on_hotkey(app: &AppHandle, msg: HotkeyMsg) {
    let controller = ctl(app);
    match msg {
        HotkeyMsg::Action(Action::Start) => controller.start(),
        HotkeyMsg::Action(Action::Stop) => controller.stop(),
        HotkeyMsg::Action(Action::HandsFree) => controller.hands_free(),
        HotkeyMsg::Recorded(shortcut) => {
            let target = controller.recording_shortcut();
            let mut settings = controller.settings();
            match target.as_deref() {
                Some("pushToTalk") if shortcut != settings.hands_free => settings.push_to_talk = shortcut,
                Some("handsFree") if shortcut != settings.push_to_talk => settings.hands_free = shortcut,
                _ => {}
            }
            controller.set_recording_shortcut(None);
            controller.update_settings(settings);
        }
        HotkeyMsg::RecordCancelled => controller.set_recording_shortcut(None),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            log::write("Kotev starting");
            let handle = app.handle().clone();
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let controller = Controller::new(handle.clone(), settings_path);
            let settings = controller.settings();
            app.manage(controller);

            // Windows are created only now, so commands from the UI always find the controller.
            for config in &app.config().app.windows {
                tauri::WebviewWindowBuilder::from_config(&handle, config)?.build()?;
            }

            tray::create(&handle)?;
            overlay::configure(&handle);

            let (tx, rx) = std::sync::mpsc::channel::<HotkeyMsg>();
            #[cfg(windows)]
            hotkeys::start(settings.push_to_talk.clone(), settings.hands_free.clone(), tx);
            #[cfg(not(windows))]
            drop((tx, settings));
            let hotkey_app = handle.clone();
            std::thread::spawn(move || {
                for msg in rx {
                    on_hotkey(&hotkey_app, msg);
                }
            });

            let started_minimized = std::env::args().any(|a| a == "--minimized");
            if !started_minimized {
                tray::show_main(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main window keeps Kotev running in the notification area.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            start_dictation,
            stop_dictation,
            toggle_dictation,
            clear_error,
            update_settings,
            get_api_key,
            save_api_key,
            remove_api_key,
            list_microphones,
            record_shortcut,
            reset_shortcuts,
            open_microphone_privacy,
            open_log_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kotev");
}
