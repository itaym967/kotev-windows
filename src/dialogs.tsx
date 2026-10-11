import { useEffect, useState } from "react";
import { api, type Snapshot } from "./api";
import { Button, Dialog } from "./components";
import { useT } from "./i18n";

export function ApiKeyDialog({ onClose, onToast }: { onClose: () => void; onToast: (msg: string) => void }) {
  const t = useT();
  const [key, setKey] = useState("");
  const [visible, setVisible] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.getApiKey().then(setKey);
  }, []);

  const save = async () => {
    try {
      await api.saveApiKey(key);
      onToast(t("המפתח נשמר", "API key saved"));
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };
  const remove = async () => {
    try {
      await api.removeApiKey();
      setKey("");
      onToast(t("המפתח הוסר", "API key removed"));
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Dialog
      title={t("מפתח API", "API key")}
      subtitle={t(
        "המפתח נשמר במנהל האישורים של Windows במחשב הזה",
        "The key is stored in Windows Credential Manager on this PC",
      )}
      onClose={onClose}
      footer={
        <>
          <Button variant="destructive" onClick={remove}>{t("הסרת המפתח", "Remove key")}</Button>
          <Button variant="primary" onClick={save} disabled={!key.trim()}>{t("שמירה", "Save")}</Button>
        </>
      }
    >
      <form className="key-field" onSubmit={(e) => { e.preventDefault(); save(); }}>
        <input
          className="input"
          dir="ltr"
          type={visible ? "text" : "password"}
          placeholder="Soniox API key"
          value={key}
          autoFocus
          spellCheck={false}
          onChange={(e) => setKey(e.target.value)}
        />
        <Button variant="ghost" onClick={() => setVisible(!visible)}>
          {visible ? t("הסתרת המפתח", "Hide API key") : t("הצגת המפתח", "Show API key")}
        </Button>
      </form>
      {error && <p className="error-text">{error}</p>}
    </Dialog>
  );
}

export function ShortcutsDialog({ snapshot, onClose }: { snapshot: Snapshot; onClose: () => void }) {
  const t = useT();
  const { settings, recordingShortcut } = snapshot;
  const close = () => {
    api.recordShortcut(null);
    onClose();
  };
  const rows = [
    {
      target: "pushToTalk" as const,
      title: t("לחיצה ודיבור", "Push to talk"),
      subtitle: t("החזק את הקיצור בזמן הדיבור", "Hold the shortcut while speaking"),
      label: settings.pushToTalk.label,
    },
    {
      target: "handsFree" as const,
      title: t("מצב ללא ידיים", "Hands-free mode"),
      subtitle: t("לחץ פעם אחת להתחלה ופעם נוספת לסיום", "Press once to start and again to stop"),
      label: settings.handsFree.label,
    },
  ];
  return (
    <Dialog
      title={t("קיצורים", "Shortcuts")}
      subtitle={t("בחר את הקיצורים להפעלת ההכתבה", "Choose the shortcuts used to start dictation")}
      onClose={close}
      footer={
        <>
          <Button onClick={() => api.resetShortcuts()}>{t("איפוס לברירת המחדל", "Reset to default")}</Button>
          <Button variant="primary" onClick={close}>{t("סיום", "Done")}</Button>
        </>
      }
    >
      {rows.map((row) => {
        const recording = recordingShortcut === row.target;
        return (
          <div className="row" key={row.target}>
            <div className="row-text">
              <div className="row-title">{row.title}</div>
              <div className="row-subtitle">{row.subtitle}</div>
            </div>
            <button
              className={`shortcut-field ${recording ? "recording" : ""}`}
              dir="ltr"
              onClick={() => api.recordShortcut(recording ? null : row.target)}
            >
              {recording ? t("לחץ על מקש או צירוף מקשים", "Press a key or key combination") : row.label}
            </button>
          </div>
        );
      })}
      <p className="muted small">
        {t(
          "Esc מבטל את ההקלטה. מקש בודד כמו Ctrl ימני מתאים במיוחד ללחיצה ודיבור.",
          "Esc cancels recording. A single key such as Right Ctrl works best for push to talk.",
        )}
      </p>
    </Dialog>
  );
}

export function MicrophoneDialog({ snapshot, onClose }: { snapshot: Snapshot; onClose: () => void }) {
  const t = useT();
  const [devices, setDevices] = useState<string[]>([]);
  useEffect(() => {
    api.listMicrophones().then(setDevices);
  }, []);
  const select = (name: string | null) =>
    api.updateSettings({ ...snapshot.settings, selectedMicrophone: name });
  const current = snapshot.settings.selectedMicrophone;
  return (
    <Dialog
      title={t("מיקרופון", "Microphone")}
      subtitle={t("בחר את המיקרופון שישמש להכתבה", "Choose the microphone used for dictation")}
      onClose={onClose}
      footer={<Button variant="primary" onClick={onClose}>{t("סיום", "Done")}</Button>}
    >
      <div className="choice-list" role="radiogroup">
        <label className={`choice ${current === null ? "selected" : ""}`}>
          <input type="radio" checked={current === null} onChange={() => select(null)} />
          <span>
            <strong>{t("בחירה אוטומטית", "Auto-select")}</strong>
            <span className="muted small">{t("משתמש בהגדרת הקלט של Windows (מומלץ)", "Uses the Windows input setting (recommended)")}</span>
          </span>
        </label>
        {devices.map((name) => (
          <label key={name} className={`choice ${current === name ? "selected" : ""}`}>
            <input type="radio" checked={current === name} onChange={() => select(name)} />
            <span>
              <strong dir="auto">{name}</strong>
              <span className="muted small">{t("מיקרופון זמין", "Available microphone")}</span>
            </span>
          </label>
        ))}
      </div>
    </Dialog>
  );
}
