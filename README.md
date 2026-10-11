# Kotev for Windows

A Windows port of [Kotev](https://github.com/Quegenx/kotev), the push-to-talk dictation app for Hebrew and English. Hold a key, speak, and the text appears in whatever app you're typing in. Kotev records the microphone, streams the audio to [Soniox](https://soniox.com/) real-time speech-to-text, and inserts the result into the focused window.

Built with Rust and [Tauri 2](https://tauri.app/). The UI is React + TypeScript.

## Features

- **Push to talk**: hold **Right Ctrl** and speak. Releasing the key stops dictation and inserts the text.
- **Hands-free**: while holding Right Ctrl, tap **Space** to lock recording on. Press Right Ctrl again to stop. Pressing **Right Ctrl + Space** also starts hands-free mode directly.
- You can choose both shortcuts in Settings → Shortcuts. A shortcut can be a single key (such as Right Ctrl) or a combination.
- Dictation in Hebrew or English, and an interface in Hebrew (RTL) or English.
- Works in editors, browsers, messaging apps and terminals. Windows Terminal, Alacritty and WezTerm get Ctrl+Shift+V; mintty and PuTTY get Shift+Insert.
- Microphone selection.
- A notification-area icon. Closing the window keeps Kotev running.
- A recording indicator that never takes focus from the app you're dictating into.
- Optional start with Windows.

## Requirements

- Windows 10 or 11 (WebView2 is built into Windows 11)
- A Soniox API key
- To build: Rust (stable, MSVC), the Visual Studio C++ Build Tools, and Node.js 20.19+ or 22.12+

## Build and run

```bash
npm install
npm run tauri dev
```

To build a release EXE:

```bash
npm run tauri build -- --no-bundle
```

The EXE is written to `src-tauri/target/release/Kotev.exe`. Running `npm run tauri build` also creates NSIS and MSI installers under `src-tauri/target/release/bundle/`. The first time, Tauri downloads the NSIS and WiX tools it needs for this.

After launching, open **Settings → API key** and enter your Soniox key.

## Tests

```bash
cd src-tauri
cargo test
```

The tests cover:
- Soniox response parsing (final and provisional tokens, `<fin>`/`<end>` markers, errors) and the config message
- The push-to-talk / hands-free state machine and the shortcut recorder
- Resampling to 16 kHz
- Which paste shortcut each terminal gets

## Privacy

- **API key**: stored in Windows Credential Manager, not in the settings file.
- **Audio**: sent directly to Soniox. There is no analytics or telemetry.
- **Clipboard**: when Kotev pastes, it saves the previous clipboard contents and restores them about half a second later. The dictated text is marked so it doesn't go into the Win+V clipboard history. Choose **Type** as the insertion method in Settings to leave the clipboard alone entirely.
- **Settings and logs**: settings are in `%APPDATA%\io.github.quegenx.kotev.win\settings.json`, logs in `%LOCALAPPDATA%\Kotev\logs\kotev.log`.

## Known limitations

- Windows doesn't let a regular app receive shortcuts from, or type into, windows running as administrator. To dictate into those, run Kotev as administrator too.
- While Kotev is running, Right Ctrl is taken by the push-to-talk shortcut. If you need it for something else, pick a different shortcut.

## Project layout

| Path | Purpose |
|---|---|
| `src-tauri/src/controller.rs` | Dictation state machine (port of `AppModel.swift`) |
| `src-tauri/src/soniox.rs` | Soniox WebSocket client |
| `src-tauri/src/audio.rs` | Microphone capture and conversion to 16 kHz mono PCM (cpal) |
| `src-tauri/src/hotkeys.rs` | Low-level keyboard hook and the push-to-talk / hands-free state machine |
| `src-tauri/src/insert.rs` | Clipboard + paste, or Unicode typing |
| `src-tauri/src/overlay.rs`, `src/overlay.tsx` | Recording indicator |
| `src-tauri/src/tray.rs` | Notification-area icon and menu |
| `src-tauri/src/settings.rs` | Saved settings and the API key |
| `src/` | Main window UI |

## License

MIT, same as the original project. See [LICENSE](LICENSE).
