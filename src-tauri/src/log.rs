//! Appends timestamped lines to %LOCALAPPDATA%\Kotev\logs\kotev.log (port of KotevLog.swift).

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOCK: Mutex<()> = Mutex::new(());

fn log_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    let dir = PathBuf::from(base).join("Kotev").join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("kotev.log"))
}

pub fn write(message: &str) {
    #[cfg(debug_assertions)]
    eprintln!("[kotev] {message}");
    let _guard = LOCK.lock();
    let Some(path) = log_path() else { return };
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{secs} {message}");
    }
}
