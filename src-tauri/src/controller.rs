//! Dictation state machine wiring audio, Soniox, insertion, overlay and UI.
//! Port of AppModel.swift.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::unbounded_channel;

use crate::audio::AudioCapture;
use crate::i18n::tr;
use crate::overlay::{self, Kind};
use crate::settings::{self, Settings};
use crate::soniox::{self, SonioxConfig, SonioxEvent};

const FINALIZE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Idle,
    Connecting,
    Listening,
    Finishing,
    Inserting,
    Error,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub phase: Phase,
    pub final_text: String,
    pub provisional_text: String,
    pub error: Option<String>,
    pub settings: Settings,
    pub has_api_key: bool,
    pub hands_free: bool,
    pub recording_shortcut: Option<String>,
}

struct Inner {
    phase: Phase,
    final_text: String,
    provisional_text: String,
    error: Option<String>,
    session: u64,
    audio: Option<AudioCapture>,
    settings: Settings,
    has_api_key: bool,
    hands_free: bool,
    recording_shortcut: Option<String>,
}

pub struct Controller {
    app: AppHandle,
    settings_path: PathBuf,
    inner: Mutex<Inner>,
}

impl Controller {
    pub fn new(app: AppHandle, settings_path: PathBuf) -> Arc<Self> {
        let settings = Settings::load(&settings_path);
        Arc::new(Self {
            app,
            settings_path,
            inner: Mutex::new(Inner {
                phase: Phase::Idle,
                final_text: String::new(),
                provisional_text: String::new(),
                error: None,
                session: 0,
                audio: None,
                settings,
                has_api_key: !settings::load_api_key().is_empty(),
                hands_free: false,
                recording_shortcut: None,
            }),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn snapshot(&self) -> Snapshot {
        let inner = self.lock();
        Snapshot {
            phase: inner.phase,
            final_text: inner.final_text.clone(),
            provisional_text: inner.provisional_text.clone(),
            error: inner.error.clone(),
            settings: inner.settings.clone(),
            has_api_key: inner.has_api_key,
            hands_free: inner.hands_free,
            recording_shortcut: inner.recording_shortcut.clone(),
        }
    }

    pub fn settings(&self) -> Settings {
        self.lock().settings.clone()
    }

    pub fn lang(&self) -> String {
        self.lock().settings.app_language.clone()
    }

    fn publish(&self) {
        let snapshot = self.snapshot();
        crate::tray::refresh(&self.app, &snapshot);
        let _ = self.app.emit("state", snapshot);
    }

    pub fn is_active(&self) -> bool {
        matches!(self.lock().phase, Phase::Connecting | Phase::Listening | Phase::Finishing | Phase::Inserting)
    }

    pub fn toggle(self: &Arc<Self>) {
        match self.lock().phase {
            Phase::Connecting | Phase::Listening => self.stop(),
            Phase::Idle | Phase::Error => self.start(),
            _ => {}
        }
    }

    pub fn start(self: &Arc<Self>) {
        let lang = self.lang();
        if self.is_active() {
            return;
        }
        let api_key = settings::load_api_key();
        if api_key.is_empty() {
            self.fail(tr(&lang, "יש להגדיר מפתח Soniox בהגדרות", "Set a Soniox API key in Settings"));
            return;
        }

        let (session, settings) = {
            let mut inner = self.lock();
            inner.session += 1;
            inner.phase = Phase::Connecting;
            inner.final_text.clear();
            inner.provisional_text.clear();
            inner.error = None;
            inner.hands_free = false;
            (inner.session, inner.settings.clone())
        };
        crate::log::write(&format!("session {session}: start"));
        overlay::show(&self.app, Kind::Busy, tr(&lang, "מתחיל…", "Starting…"));
        self.publish();

        // Capture starts before the socket connects; chunks wait in the channel.
        let (audio_tx, audio_rx) = unbounded_channel::<Vec<u8>>();
        let level_app = self.app.clone();
        let capture = AudioCapture::start(
            settings.selected_microphone.clone(),
            move |chunk| {
                let _ = audio_tx.send(chunk);
            },
            move |level| overlay::level(&level_app, level),
        );
        match capture {
            Ok(capture) => self.lock().audio = Some(capture),
            Err(e) => {
                crate::log::write(&format!("microphone error: {e}"));
                self.fail(format!("{}: {e}", tr(&lang, "שגיאת מיקרופון", "Microphone error")));
                return;
            }
        }

        let this = Arc::clone(self);
        let config = SonioxConfig { api_key, language_hint: settings.language_hint };
        tauri::async_runtime::spawn(soniox::run(config, audio_rx, move |event| {
            this.on_soniox(session, event)
        }));
    }

    pub fn stop(self: &Arc<Self>) {
        let lang = self.lang();
        let (audio, session) = {
            let mut inner = self.lock();
            if !matches!(inner.phase, Phase::Connecting | Phase::Listening) {
                return;
            }
            inner.phase = Phase::Finishing;
            (inner.audio.take(), inner.session)
        };
        crate::log::write(&format!("session {session}: stop"));
        // Dropping the stream closes the audio channel, which triggers finalization.
        if let Some(audio) = audio {
            audio.stop();
        }
        overlay::show(&self.app, Kind::Busy, tr(&lang, "מסיים…", "Finishing…"));
        self.publish();

        let this = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(FINALIZE_TIMEOUT).await;
            this.finish_after_timeout(session);
        });
    }

    /// Push-to-talk turned into hands-free: keep recording after the key is released.
    pub fn hands_free(&self) {
        let lang = self.lang();
        {
            let mut inner = self.lock();
            if !matches!(inner.phase, Phase::Connecting | Phase::Listening) {
                return;
            }
            inner.hands_free = true;
        }
        overlay::show(&self.app, Kind::Listening, tr(&lang, "מצב ללא ידיים…", "Hands-free mode…"));
        self.publish();
    }

    fn on_soniox(self: &Arc<Self>, session: u64, event: SonioxEvent) {
        let lang = self.lang();
        {
            let mut inner = self.lock();
            if inner.session != session {
                return;
            }
            match &event {
                SonioxEvent::Connected => {
                    if inner.phase == Phase::Connecting {
                        inner.phase = Phase::Listening;
                    }
                }
                SonioxEvent::Transcript { final_text, provisional } => {
                    inner.final_text.push_str(final_text);
                    inner.provisional_text = provisional.clone();
                }
                _ => {}
            }
        }
        match event {
            SonioxEvent::Connected => {
                if self.lock().phase == Phase::Listening {
                    let hands_free = self.lock().hands_free;
                    let label = if hands_free {
                        tr(&lang, "מצב ללא ידיים…", "Hands-free mode…")
                    } else {
                        tr(&lang, "מקשיב…", "Listening…")
                    };
                    overlay::show(&self.app, Kind::Listening, label);
                }
                self.publish();
            }
            SonioxEvent::Transcript { .. } => self.publish(),
            SonioxEvent::Finished => self.finish(session),
            SonioxEvent::Error(e) => {
                crate::log::write(&format!("session {session}: soniox error: {e}"));
                self.fail(format!("{}: {e}", tr(&lang, "שגיאת תמלול", "Transcription error")));
            }
        }
    }

    fn finish_after_timeout(self: &Arc<Self>, session: u64) {
        let (still_finishing, has_text) = {
            let inner = self.lock();
            (inner.session == session && inner.phase == Phase::Finishing, !inner.final_text.trim().is_empty())
        };
        if !still_finishing {
            return;
        }
        crate::log::write(&format!("session {session}: finalize timed out"));
        if has_text {
            self.finish(session);
        } else {
            let lang = self.lang();
            self.fail(tr(&lang, "השלמת התמלול נמשכה זמן רב מדי", "Transcription took too long to finish"));
        }
    }

    fn finish(self: &Arc<Self>, session: u64) {
        let (text, settings) = {
            let mut inner = self.lock();
            if inner.session != session || inner.phase == Phase::Inserting {
                return;
            }
            // A session stopped by a still-held push-to-talk key may get here while listening.
            if let Some(audio) = inner.audio.take() {
                audio.stop();
            }
            let provisional = std::mem::take(&mut inner.provisional_text);
            inner.final_text.push_str(&provisional);
            inner.phase = Phase::Inserting;
            inner.hands_free = false;
            (inner.final_text.trim().to_string(), inner.settings.clone())
        };
        crate::hotkeys_reset();
        self.publish();

        let this = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            let lang = settings.app_language.clone();
            let result = if text.is_empty() || !settings.insert_automatically {
                Ok(())
            } else {
                crate::insert_text(&text, settings.insert_mode)
            };
            {
                let mut inner = this.lock();
                if inner.session == session {
                    inner.phase = Phase::Idle;
                }
            }
            match result {
                Ok(()) => {
                    crate::log::write(&format!("session {session}: done ({} chars)", text.chars().count()));
                    overlay::hide(&this.app);
                    this.publish();
                }
                Err(e) => this.fail(format!("{}: {e}", tr(&lang, "לא ניתן להכניס את הטקסט", "Could not insert text"))),
            }
        });
    }

    fn fail(self: &Arc<Self>, message: String) {
        crate::log::write(&format!("failed: {message}"));
        let audio = {
            let mut inner = self.lock();
            inner.session += 1;
            inner.phase = Phase::Error;
            inner.error = Some(message.clone());
            inner.hands_free = false;
            inner.audio.take()
        };
        if let Some(audio) = audio {
            audio.stop();
        }
        crate::hotkeys_reset();
        overlay::show(&self.app, Kind::Error, message);
        self.publish();
    }

    pub fn clear_error(&self) {
        {
            let mut inner = self.lock();
            inner.error = None;
            if inner.phase == Phase::Error {
                inner.phase = Phase::Idle;
            }
        }
        self.publish();
    }

    pub fn update_settings(&self, settings: Settings) {
        settings.save(&self.settings_path);
        let (ptt, hf) = (settings.push_to_talk.clone(), settings.hands_free.clone());
        self.lock().settings = settings;
        crate::hotkeys_update(ptt, hf);
        self.publish();
    }

    pub fn set_has_api_key(&self, has: bool) {
        self.lock().has_api_key = has;
        self.publish();
    }

    pub fn set_recording_shortcut(&self, target: Option<String>) {
        self.lock().recording_shortcut = target;
        self.publish();
    }

    pub fn recording_shortcut(&self) -> Option<String> {
        self.lock().recording_shortcut.clone()
    }
}
