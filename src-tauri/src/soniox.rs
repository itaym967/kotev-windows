//! Soniox real-time speech-to-text client.
//! Port of SonioxService.swift / Models.swift from the macOS app.

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub const ENDPOINT: &str = "wss://stt-rt.soniox.com/transcribe-websocket";
pub const MODEL: &str = "stt-rt-v5";
pub const SAMPLE_RATE: u32 = 16_000;

/// Terms that bias recognition toward app and product names users dictate often.
pub const CONTEXT_TERMS: &[&str] = &[
    "VS Code", "Cursor", "Terminal", "WhatsApp", "Claude", "Claude Code", "Codex", "ChatGPT",
];

/// Soniox recommends roughly 200 ms of trailing silence before finalization.
const TRAILING_SILENCE_BYTES: usize = (SAMPLE_RATE as usize) * 2 / 5;

pub struct SonioxConfig {
    pub api_key: String,
    pub language_hint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SonioxEvent {
    /// The socket is open and the configuration was accepted for sending.
    Connected,
    /// `final_text` is new final text to append; `provisional` replaces the previous provisional text.
    Transcript { final_text: String, provisional: String },
    Finished,
    Error(String),
}

pub fn config_message(cfg: &SonioxConfig) -> String {
    let mut config = json!({
        "api_key": cfg.api_key,
        "model": MODEL,
        "audio_format": "pcm_s16le",
        "sample_rate": SAMPLE_RATE,
        "num_channels": 1,
        "language_hints": [cfg.language_hint],
        "language_hints_strict": false,
        "enable_language_identification": true,
        "enable_endpoint_detection": true,
        "max_endpoint_delay_ms": 900,
    });
    if !CONTEXT_TERMS.is_empty() {
        config["context"] = json!({ "terms": CONTEXT_TERMS });
    }
    config.to_string()
}

#[derive(Debug, Deserialize)]
struct Token {
    text: String,
    #[serde(default)]
    is_final: bool,
}

#[derive(Debug, Deserialize)]
struct Response {
    tokens: Option<Vec<Token>>,
    finished: Option<bool>,
    error_message: Option<String>,
    error_type: Option<String>,
    error_code: Option<serde_json::Value>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Parsed {
    pub final_text: String,
    pub provisional: String,
    pub done: bool,
}

pub fn parse_response(text: &str) -> Result<Parsed, String> {
    let response: Response =
        serde_json::from_str(text).map_err(|e| format!("invalid response: {e}"))?;
    if let Some(message) = response.error_message {
        let kind = response
            .error_type
            .or_else(|| response.error_code.map(|c| c.to_string()))
            .unwrap_or_else(|| "error".into());
        return Err(format!("{kind}: {message}"));
    }
    let tokens = response.tokens.unwrap_or_default();
    let mut parsed = Parsed {
        done: response.finished == Some(true) || tokens.iter().any(|t| t.text == "<fin>"),
        ..Default::default()
    };
    for token in tokens.iter().filter(|t| t.text != "<fin>" && t.text != "<end>") {
        if token.is_final {
            parsed.final_text.push_str(&token.text);
        } else {
            parsed.provisional.push_str(&token.text);
        }
    }
    Ok(parsed)
}

/// Runs one transcription session. Audio chunks (16 kHz mono s16le) are read from `audio`
/// until the sender is dropped; chunks produced before the socket is ready simply wait in
/// the channel, so the first syllable is not lost. Always ends with `Finished` or `Error`.
pub async fn run(
    cfg: SonioxConfig,
    mut audio: UnboundedReceiver<Vec<u8>>,
    on_event: impl Fn(SonioxEvent) + Send + 'static,
) {
    if let Err(e) = session(cfg, &mut audio, &on_event).await {
        on_event(SonioxEvent::Error(e));
    }
}

async fn session(
    cfg: SonioxConfig,
    audio: &mut UnboundedReceiver<Vec<u8>>,
    on_event: &(impl Fn(SonioxEvent) + Send),
) -> Result<(), String> {
    let (ws, _) = connect_async(ENDPOINT).await.map_err(|e| e.to_string())?;
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::Text(config_message(&cfg).into()))
        .await
        .map_err(|e| e.to_string())?;
    on_event(SonioxEvent::Connected);

    let mut audio_open = true;
    let (mut bytes_sent, mut responses, mut final_chars) = (0usize, 0usize, 0usize);
    loop {
        tokio::select! {
            chunk = audio.recv(), if audio_open => match chunk {
                Some(bytes) => {
                    bytes_sent += bytes.len();
                    sink.send(Message::Binary(bytes.into())).await.map_err(|e| e.to_string())?
                }
                None => {
                    audio_open = false;
                    crate::log::write(&format!(
                        "soniox: audio ended after {bytes_sent} bytes ({:.1} s), finalizing",
                        bytes_sent as f64 / (SAMPLE_RATE as f64 * 2.0)
                    ));
                    sink.send(Message::Binary(vec![0u8; TRAILING_SILENCE_BYTES].into()))
                        .await.map_err(|e| e.to_string())?;
                    sink.send(Message::Text(r#"{"type":"finalize"}"#.to_string().into()))
                        .await.map_err(|e| e.to_string())?;
                }
            },
            message = stream.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    responses += 1;
                    let parsed = parse_response(text.as_ref())?;
                    final_chars += parsed.final_text.chars().count();
                    if !parsed.final_text.is_empty() || !parsed.provisional.is_empty() {
                        on_event(SonioxEvent::Transcript {
                            final_text: parsed.final_text,
                            provisional: parsed.provisional,
                        });
                    }
                    if parsed.done {
                        crate::log::write(&format!(
                            "soniox: finished after {responses} responses, {final_chars} final chars"
                        ));
                        let _ = sink.close().await;
                        on_event(SonioxEvent::Finished);
                        return Ok(());
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    return Err("connection closed by the transcription service".into());
                }
                Some(Err(e)) => return Err(e.to_string()),
                Some(Ok(_)) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_final_hebrew_token() {
        let parsed =
            parse_response(r#"{"tokens":[{"text":"שלום","is_final":true}],"finished":true}"#)
                .unwrap();
        assert_eq!(parsed.final_text, "שלום");
        assert!(parsed.done);
    }

    #[test]
    fn splits_final_and_provisional_and_drops_markers() {
        let parsed = parse_response(
            r#"{"tokens":[{"text":"Hello","is_final":true},{"text":"<end>","is_final":true},{"text":" wor","is_final":false}]}"#,
        )
        .unwrap();
        assert_eq!(parsed.final_text, "Hello");
        assert_eq!(parsed.provisional, " wor");
        assert!(!parsed.done, "<end> must not end the session");
    }

    #[test]
    fn fin_token_ends_session() {
        let parsed =
            parse_response(r#"{"tokens":[{"text":"<fin>","is_final":true}]}"#).unwrap();
        assert!(parsed.done);
        assert!(parsed.final_text.is_empty());
    }

    #[test]
    fn reports_errors() {
        let err = parse_response(
            r#"{"error_code":401,"error_type":"unauthenticated","error_message":"Invalid API key"}"#,
        )
        .unwrap_err();
        assert_eq!(err, "unauthenticated: Invalid API key");
    }

    #[test]
    fn config_contains_required_fields() {
        let msg = config_message(&SonioxConfig { api_key: "k".into(), language_hint: "he".into() });
        let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["model"], MODEL);
        assert_eq!(v["sample_rate"], 16000);
        assert_eq!(v["language_hints"][0], "he");
        assert_eq!(v["context"]["terms"][0], "VS Code");
    }

    /// Streams a 16 kHz mono 16-bit WAV to Soniox with the stored API key.
    /// Run with: KOTEV_TEST_WAV=path cargo test live_soniox -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_soniox() {
        let path = std::env::var("KOTEV_TEST_WAV").expect("KOTEV_TEST_WAV");
        let wav = std::fs::read(path).unwrap();
        let pcm = wav[44..].to_vec();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        for chunk in pcm.chunks(3200) {
            tx.send(chunk.to_vec()).unwrap();
        }
        drop(tx);
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = events.clone();
        let cfg = SonioxConfig { api_key: crate::settings::load_api_key(), language_hint: "en".into() };
        assert!(!cfg.api_key.is_empty(), "no API key stored");
        run(cfg, rx, move |e| sink.lock().unwrap().push(e)).await;
        let events = events.lock().unwrap();
        let text: String = events
            .iter()
            .filter_map(|e| match e {
                SonioxEvent::Transcript { final_text, .. } => Some(final_text.as_str()),
                _ => None,
            })
            .collect();
        println!("events: {:?}", events.last());
        println!("transcript: {text}");
        assert!(!text.trim().is_empty());
    }
}
