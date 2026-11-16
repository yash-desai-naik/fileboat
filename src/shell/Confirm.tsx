import { useEffect, useRef, useSyncExternalStore, type ReactNode } from "react";
import { Dialog } from "./Dialog";

interface Ask {
  title: string;
  body: ReactNode;
  /** The button that goes ahead ("Delete"); Cancel is always there. */
  confirm: string;
  /** Red: the action can't be undone. */
  danger?: boolean;
  resolve: (ok: boolean) => void;
}

let current: Ask | null = null;
const listeners = new Set<() => void>();
const set = (a: Ask | null) => {
  current = a;
  for (const l of listeners) l();
};

/** Ask the user to confirm something; resolves true for the confirm button, false otherwise. */
export function askConfirm(options: Omit<Ask, "resolve">): Promise<boolean> {
  current?.resolve(false);
  return new Promise((resolve) => set({ ...options, resolve }));
}

/** Rendered once by the app: shows the question being asked, if any. */
export function ConfirmHost() {
  const ask = useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
  const okRef = useRef<HTMLButtonElement>(null);
  useEffect(() => okRef.current?.focus(), [ask]);
  if (!ask) return null;
  const answer = (ok: boolean) => {
    set(null);
    ask.resolve(ok);
  };
  return (
    <Dialog
      title={ask.title}
      onClose={() => answer(false)}
      width={460}
      footer={
        <>
          <span className="spacer" />
          <button className="btn" onClick={() => answer(false)}>
            Cancel
          </button>
          <button
            ref={okRef}
            className={`btn ${ask.danger ? "danger" : "primary"}`}
            onClick={() => answer(true)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                answer(true);
              }
            }}
          >
            {ask.confirm}
          </button>
        </>
      }
    >
      {ask.body}
    </Dialog>
  );
}
