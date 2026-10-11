import { createContext, useContext } from "react";
import type { Lang, Phase } from "./api";

export const LangContext = createContext<Lang>("he");

/** Port of `uiText(he, en)` from AppLanguage.swift. */
export function useT() {
  const lang = useContext(LangContext);
  return (he: string, en: string) => (lang === "en" ? en : he);
}

export function phaseText(t: (he: string, en: string) => string, phase: Phase): string {
  switch (phase) {
    case "idle":
      return t("מוכן להכתבה", "Ready to dictate");
    case "connecting":
      return t("מתחבר לשירות התמלול", "Connecting to transcription");
    case "listening":
      return t("מקשיב…", "Listening…");
    case "finishing":
      return t("מסיים את התמלול…", "Finishing transcription…");
    case "inserting":
      return t("מכניס את הטקסט…", "Inserting text…");
    case "error":
      return t("ההכתבה נעצרה", "Dictation stopped");
  }
}
