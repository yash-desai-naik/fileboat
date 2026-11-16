import { useSyncExternalStore } from "react";

interface Toast {
  id: number;
  text: string;
  kind: "info" | "error";
}

let toasts: Toast[] = [];
let next = 0;
const listeners = new Set<() => void>();
const emit = () => {
  for (const l of listeners) l();
};

/** A short message in the corner ("Undid: Rename “a” to “b”"); errors stay a little longer. */
export function showToast(text: string, kind: Toast["kind"] = "info") {
  const toast = { id: ++next, text, kind };
  toasts = [...toasts.slice(-2), toast];
  emit();
  window.setTimeout(
    () => {
      toasts = toasts.filter((t) => t !== toast);
      emit();
    },
    kind === "error" ? 8000 : 3500,
  );
}

export function Toasts() {
  const list = useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => toasts,
  );
  if (!list.length) return null;
  return (
    <div className="toasts" role="status" aria-live="polite">
      {list.map((t) => (
        <div key={t.id} className={`toast ${t.kind}`}>
          {t.text}
        </div>
      ))}
    </div>
  );
}
