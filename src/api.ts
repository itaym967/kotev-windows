import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

export type Phase = "idle" | "connecting" | "listening" | "finishing" | "inserting" | "error";
export type Lang = "he" | "en";

export interface Shortcut {
  vk: number;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  win: boolean;
  modifierOnly: boolean;
  label: string;
}

export interface Settings {
  appLanguage: Lang;
  languageHint: Lang;
  insertAutomatically: boolean;
  insertMode: "paste" | "type";
  selectedMicrophone: string | null;
  pushToTalk: Shortcut;
  handsFree: Shortcut;
}

export interface Snapshot {
  phase: Phase;
  finalText: string;
  provisionalText: string;
  error: string | null;
  settings: Settings;
  hasApiKey: boolean;
  handsFree: boolean;
  recordingShortcut: "pushToTalk" | "handsFree" | null;
}

export const api = {
  getState: () => invoke<Snapshot>("get_state"),
  start: () => invoke("start_dictation"),
  stop: () => invoke("stop_dictation"),
  toggle: () => invoke("toggle_dictation"),
  clearError: () => invoke("clear_error"),
  updateSettings: (settings: Settings) => invoke("update_settings", { settings }),
  getApiKey: () => invoke<string>("get_api_key"),
  saveApiKey: (key: string) => invoke("save_api_key", { key }),
  removeApiKey: () => invoke("remove_api_key"),
  listMicrophones: () => invoke<string[]>("list_microphones"),
  recordShortcut: (target: "pushToTalk" | "handsFree" | null) => invoke("record_shortcut", { target }),
  resetShortcuts: () => invoke("reset_shortcuts"),
  openMicrophonePrivacy: () => invoke("open_microphone_privacy"),
  openLogFolder: () => invoke("open_log_folder"),
};

export function useSnapshot(): Snapshot | null {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  useEffect(() => {
    api.getState().then(setSnapshot);
    const unlisten = listen<Snapshot>("state", (event) => setSnapshot(event.payload));
    return () => {
      unlisten.then((off) => off());
    };
  }, []);
  return snapshot;
}

export const isActive = (phase: Phase) => phase === "connecting" || phase === "listening";
