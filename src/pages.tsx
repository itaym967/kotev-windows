import { useEffect, useState } from "react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { api, isActive, type Lang, type Snapshot } from "./api";
import { Badge, Button, Card, Icon, Row, Select } from "./components";
import { ApiKeyDialog, MicrophoneDialog, ShortcutsDialog } from "./dialogs";
import { phaseText, useT } from "./i18n";

export function DictationPage({ snapshot, onToast }: { snapshot: Snapshot; onToast: (msg: string) => void }) {
  const t = useT();
  const active = isActive(snapshot.phase);
  const transcript = snapshot.finalText + snapshot.provisionalText;
  const ptt = snapshot.settings.pushToTalk.label;
  const hint = !active
    ? t(`החזק ${ptt} ודבר בכל יישום`, `Hold ${ptt} and speak in any app`)
    : snapshot.handsFree
      ? t(`לחץ ${ptt} כדי לסיים`, `Press ${ptt} to stop`)
      : t("שחרור הקיצור יסיים את ההכתבה", "Release the shortcut to stop dictating");

  const copy = async () => {
    await navigator.clipboard.writeText(transcript);
    onToast(t("הועתק", "Copied"));
  };

  return (
    <div className="page">
      <Card className="hero">
        <div className="hero-text">
          <div className="hero-phase">{phaseText(t, snapshot.phase)}</div>
          <div className="hero-hint"><bdi>{hint}</bdi></div>
        </div>
        <Button variant="light" onClick={() => (active ? api.stop() : api.start())}
          disabled={snapshot.phase === "finishing" || snapshot.phase === "inserting"}>
          {active ? t("סיום", "Stop") : t("התחלה", "Start")}
        </Button>
        <div className={`hero-icon ${active ? "live" : ""}`}><Icon name="mic" size={30} /></div>
      </Card>

      <Card>
        <div className="card-header">
          <h2>{t("תמלול", "Transcript")}</h2>
          <button className="icon-btn" onClick={copy} disabled={!transcript} title={t("העתקת התמלול", "Copy transcript")}>
            <Icon name="copy" />
          </button>
        </div>
        <div className="transcript" dir="auto">
          {transcript ? (
            <>
              {snapshot.finalText}
              <span className="provisional">{snapshot.provisionalText}</span>
            </>
          ) : (
            <span className="muted">{t("התמלול יופיע כאן בזמן ההכתבה", "Your transcript will appear here while dictating")}</span>
          )}
        </div>
      </Card>
    </div>
  );
}

export function PermissionsPage() {
  const t = useT();
  const [autostart, setAutostart] = useState<boolean | null>(null);
  useEffect(() => {
    isEnabled().then(setAutostart).catch(() => setAutostart(false));
  }, []);
  const toggleAutostart = async () => {
    if (autostart) await disable();
    else await enable();
    setAutostart(await isEnabled());
  };
  return (
    <div className="page">
      <p className="page-subtitle">
        {t("ההרשאות וההגדרות הדרושות להפעלה מכל יישום ב־Windows", "Permissions and options for working in every app on Windows")}
      </p>
      <Card>
        <Row title={t("מיקרופון", "Microphone")}
          subtitle={t("ודא ש־Windows מאפשר לאפליקציות שולחן עבודה לגשת למיקרופון", "Make sure Windows lets desktop apps access the microphone")}>
          <Button onClick={() => api.openMicrophonePrivacy()}>{t("פתיחת הגדרות פרטיות", "Open privacy settings")}</Button>
        </Row>
        <Row title={t("הפעלה עם Windows", "Start with Windows")}
          subtitle={t("כותב יופעל ברקע באזור ההודעות", "Kotev starts in the background in the notification area")}>
          <label className="switch">
            <input type="checkbox" checked={!!autostart} disabled={autostart === null} onChange={toggleAutostart} />
            <span />
          </label>
        </Row>
        <Row title={t("יישומים עם הרשאות מנהל", "Apps running as administrator")}
          subtitle={t(
            "Windows חוסם קיצורים והכנסת טקסט לחלונות שרצים כמנהל, אלא אם כותב רץ גם הוא כמנהל",
            "Windows blocks shortcuts and text insertion into elevated windows unless Kotev also runs as administrator",
          )} />
        <Row title={t("יומן", "Log")} subtitle={t("לאבחון תקלות", "For troubleshooting")}>
          <Button onClick={() => api.openLogFolder()}>{t("פתיחת התיקייה", "Open folder")}</Button>
        </Row>
      </Card>
    </div>
  );
}

type DialogKind = "shortcuts" | "microphone" | "apiKey" | null;

export function SettingsPage({ snapshot, onToast }: { snapshot: Snapshot; onToast: (msg: string) => void }) {
  const t = useT();
  const [dialog, setDialog] = useState<DialogKind>(null);
  const { settings } = snapshot;
  const update = (patch: Partial<typeof settings>) => api.updateSettings({ ...settings, ...patch });
  const languages: { value: Lang; label: string }[] = [
    { value: "he", label: t("עברית", "Hebrew") },
    { value: "en", label: t("אנגלית", "English") },
  ];

  return (
    <div className="page">
      <h2 className="section-title">{t("כללי", "General")}</h2>
      <Card>
        <Row title={t("קיצורים", "Shortcuts")}
          subtitle={<bdi>{t(`החזק ${settings.pushToTalk.label} ודבר`, `Hold ${settings.pushToTalk.label} and speak`)}</bdi>}>
          <Button onClick={() => setDialog("shortcuts")}>{t("שינוי", "Change")}</Button>
        </Row>
        <Row title={t("מיקרופון", "Microphone")}
          subtitle={<bdi>{settings.selectedMicrophone ?? t("בחירה אוטומטית (מומלץ)", "Auto-select (recommended)")}</bdi>}>
          <Button onClick={() => setDialog("microphone")}>{t("שינוי", "Change")}</Button>
        </Row>
        <Row
          title={<>{t("מפתח API", "API key")} <Badge ok={snapshot.hasApiKey}>{snapshot.hasApiKey ? t("מחובר", "Connected") : t("לא מחובר", "Not connected")}</Badge></>}
          subtitle={t("נדרש לצורך תמלול", "Required for transcription")}>
          <Button onClick={() => setDialog("apiKey")}>{snapshot.hasApiKey ? t("שינוי", "Change") : t("הגדרה", "Set up")}</Button>
        </Row>
        <Row title={t("שפת הכתבה", "Dictation language")}>
          <Select label={t("שפת הכתבה", "Dictation language")} value={settings.languageHint} options={languages}
            onChange={(languageHint) => update({ languageHint })} />
        </Row>
        <Row title={t("שפת היישום", "App language")} subtitle={t("שפת הממשק המועדפת", "Choose the interface language")}>
          <Select label={t("שפת היישום", "App language")} value={settings.appLanguage}
            options={[{ value: "he", label: "עברית" }, { value: "en", label: "English" }]}
            onChange={(appLanguage) => update({ appLanguage })} />
        </Row>
        <Row title={t("שיטת הכנסה", "Insertion method")}
          subtitle={t("הדבקה מהירה ומשחזרת את הלוח; הקלדה לא נוגעת בלוח", "Paste is fast and restores the clipboard; typing leaves the clipboard untouched")}>
          <Select label={t("שיטת הכנסה", "Insertion method")} value={settings.insertMode}
            options={[{ value: "paste", label: t("הדבקה", "Paste") }, { value: "type", label: t("הקלדה", "Type") }]}
            onChange={(insertMode) => update({ insertMode })} />
        </Row>
        <Row title={t("הכנסה אוטומטית", "Insert automatically")}
          subtitle={t("הכנסת הטקסט ליישום הפעיל בסיום ההכתבה", "Insert the text into the active app when dictation ends")}>
          <label className="switch">
            <input type="checkbox" checked={settings.insertAutomatically}
              onChange={(e) => update({ insertAutomatically: e.target.checked })} />
            <span />
          </label>
        </Row>
      </Card>

      {dialog === "shortcuts" && <ShortcutsDialog snapshot={snapshot} onClose={() => setDialog(null)} />}
      {dialog === "microphone" && <MicrophoneDialog snapshot={snapshot} onClose={() => setDialog(null)} />}
      {dialog === "apiKey" && <ApiKeyDialog onClose={() => setDialog(null)} onToast={onToast} />}
    </div>
  );
}
