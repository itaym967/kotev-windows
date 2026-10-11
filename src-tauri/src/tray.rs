//! Notification-area icon and menu (port of the macOS menu-bar extra).

use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::controller::{Phase, Snapshot};
use crate::i18n::tr;

pub struct Tray {
    icon: TrayIcon<Wry>,
    toggle: MenuItem<Wry>,
    open: MenuItem<Wry>,
    quit: MenuItem<Wry>,
    normal: Image<'static>,
    recording: Image<'static>,
    last: Mutex<Option<(String, bool)>>,
}

/// The app icon with a red dot in the bottom-right corner.
fn recording_variant(icon: &Image<'_>) -> Image<'static> {
    let (w, h) = (icon.width() as i32, icon.height() as i32);
    let mut rgba = icon.rgba().to_vec();
    let radius = w.min(h) / 4;
    let (cx, cy) = (w - radius - 1, h - radius - 1);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x - cx, y - cy);
            if dx * dx + dy * dy <= radius * radius {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[0xE5, 0x2D, 0x27, 0xFF]);
            }
        }
    }
    Image::new_owned(rgba, w as u32, h as u32)
}

pub fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&toggle, &open, &separator, &quit])?;

    let base = app.default_window_icon().expect("bundle icon");
    let normal = Image::new_owned(base.rgba().to_vec(), base.width(), base.height());
    let recording = recording_variant(&normal);

    let icon = TrayIconBuilder::with_id("main")
        .icon(normal.clone())
        .tooltip("Kotev")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => crate::ctl(app).toggle(),
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(Tray { icon, toggle, open, quit, normal, recording, last: Mutex::new(None) });
    Ok(())
}

pub fn refresh(app: &AppHandle, snapshot: &Snapshot) {
    let Some(tray) = app.try_state::<Tray>() else { return };
    let lang = snapshot.settings.app_language.clone();
    let active = matches!(snapshot.phase, Phase::Connecting | Phase::Listening);
    let mut last = tray.last.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_ref() == Some(&(lang.clone(), active)) {
        return;
    }
    *last = Some((lang.clone(), active));
    let _ = tray.toggle.set_text(if active {
        tr(&lang, "עצירת ההכתבה", "Stop dictation")
    } else {
        tr(&lang, "התחלת הכתבה", "Start dictation")
    });
    let _ = tray.open.set_text(tr(&lang, "פתיחת כותב", "Open Kotev"));
    let _ = tray.quit.set_text(tr(&lang, "יציאה מכותב", "Quit Kotev"));
    let _ = tray.icon.set_icon(Some(if active { tray.recording.clone() } else { tray.normal.clone() }));
    let _ = tray.icon.set_tooltip(Some(tr(&lang, "כותב", "Kotev")));
}
