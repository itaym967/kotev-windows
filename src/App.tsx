import { useEffect, useRef, useState } from "react";
import { api, useSnapshot } from "./api";
import { Icon } from "./components";
import { LangContext, useT } from "./i18n";
import { DictationPage, PermissionsPage, SettingsPage } from "./pages";

type Page = "dictation" | "permissions" | "settings";

export default function App() {
  const snapshot = useSnapshot();
  const lang = snapshot?.settings.appLanguage ?? "he";

  useEffect(() => {
    document.documentElement.lang = lang;
    document.documentElement.dir = lang === "he" ? "rtl" : "ltr";
    document.title = lang === "he" ? "כותב" : "Kotev";
  }, [lang]);

  if (!snapshot) return null;
  return (
    <LangContext.Provider value={lang}>
      <Shell snapshot={snapshot} />
    </LangContext.Provider>
  );
}

function Shell({ snapshot }: { snapshot: NonNullable<ReturnType<typeof useSnapshot>> }) {
  const t = useT();
  const [page, setPage] = useState<Page>("dictation");
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<number>(undefined);

  const showToast = (message: string) => {
    setToast(message);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 1800);
  };

  const transcript = snapshot.finalText + snapshot.provisionalText;
  const nav = (id: Page, icon: "wave" | "shield" | "gear", label: string) => (
    <button className={`nav-item ${page === id ? "active" : ""}`} onClick={() => setPage(id)}>
      <Icon name={icon} />
      <span>{label}</span>
    </button>
  );
  const titles: Record<Page, string> = {
    dictation: t("הכתבה", "Dictation"),
    permissions: t("הרשאות", "Permissions"),
    settings: t("הגדרות", "Settings"),
  };

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <img src="/app-icon.png" alt="" width={32} height={32} />
          <span>{t("כותב", "Kotev")}</span>
        </div>
        <nav>{nav("dictation", "wave", titles.dictation)}</nav>
        <nav className="nav-bottom">
          {nav("permissions", "shield", titles.permissions)}
          {nav("settings", "gear", titles.settings)}
        </nav>
      </aside>

      <main className="content">
        {snapshot.error && (
          <div className="banner" role="alert">
            <span>{snapshot.error}</span>
            <div className="banner-actions">
              {transcript && (
                <button className="link-btn" onClick={() => navigator.clipboard.writeText(transcript).then(() => showToast(t("הועתק", "Copied")))}>
                  {t("העתקת התמלול", "Copy transcript")}
                </button>
              )}
              <button className="icon-btn" onClick={() => api.clearError()} aria-label={t("סגירה", "Close")}>
                <Icon name="close" size={16} />
              </button>
            </div>
          </div>
        )}
        <h1 className="page-title">{titles[page]}</h1>
        {page === "dictation" && <DictationPage snapshot={snapshot} onToast={showToast} />}
        {page === "permissions" && <PermissionsPage />}
        {page === "settings" && <SettingsPage snapshot={snapshot} onToast={showToast} />}
      </main>

      {toast && <div className="toast"><Icon name="check" size={16} />{toast}</div>}
    </div>
  );
}
