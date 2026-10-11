import { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import "./overlay.css";

type Kind = "listening" | "busy" | "error";

function Overlay() {
  const [state, setState] = useState<{ kind: Kind; label: string }>({ kind: "busy", label: "" });
  const [level, setLevel] = useState(0);

  useEffect(() => {
    const offs = [
      listen<{ kind: Kind; label: string }>("overlay", (e) => {
        setState(e.payload);
        setLevel(0);
      }),
      listen<number>("overlay-level", (e) => setLevel(e.payload)),
    ];
    return () => offs.forEach((p) => p.then((off) => off()));
  }, []);

  const ring = state.kind === "listening" ? 2 + level * 8 : 0;
  return (
    <div className={`pill ${state.kind}`} dir="rtl">
      <span className="dot" style={{ boxShadow: `0 0 0 ${ring}px rgba(229, 45, 39, 0.18)` }} />
      <span className="label" dir="auto">{state.label}</span>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<Overlay />);
