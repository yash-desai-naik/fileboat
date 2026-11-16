import { useAppearance } from "./lib/appearance";
import { useSettings } from "./lib/settings";

export function App() {
  const settings = useSettings();
  useAppearance(settings);

  return (
    <main style={{ padding: 24, height: "100%", background: "var(--bg)" }}>
      <h1 style={{ fontSize: "var(--fs-xl)", margin: "0 0 4px" }}>Fileboat</h1>
      <p className="muted">A small, fast file explorer for developers.</p>
    </main>
  );
}
