//! Hebrew/English strings used by the backend (tray menu, overlay, errors).
//! Port of the `uiText(he, en)` helper in AppLanguage.swift.

pub fn tr(lang: &str, he: &str, en: &str) -> String {
    if lang == "en" { en } else { he }.to_string()
}
