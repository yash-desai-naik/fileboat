import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";
import { showToast } from "../shell/Toast";

/** A copy or move, as src-tauri/src/jobs.rs reports it. */
export interface Job {
  id: string;
  kind: "copy" | "move";
  what: string;
  dest: string;
  state: "running" | "conflict" | "done" | "failed" | "cancelled";
  progress: { doneBytes: number; totalBytes: number; doneItems: number; totalItems: number; current: string; counting: boolean };
  conflict: {
    name: string;
    folder: string;
    isFolder: boolean;
    sourceSize: number;
    targetSize: number;
    sourceModified: number;
    targetModified: number;
  } | null;
  message: string | null;
}

let jobs: Job[] = [];
const listeners = new Set<() => void>();
const emit = () => {
  for (const l of listeners) l();
};

function put(job: Job) {
  const i = jobs.findIndex((j) => j.id === job.id);
  jobs = i < 0 ? [...jobs, job] : jobs.map((j) => (j.id === job.id ? job : j));
  emit();
}

function drop(id: string) {
  jobs = jobs.filter((j) => j.id !== id);
  emit();
}

void listen<Job>("job", (e) => {
  const job = e.payload;
  put(job);
  if (job.state === "done" || job.state === "cancelled") {
    // Finished: say so, and let the card go.
    if (job.message) showToast(job.message);
    window.setTimeout(() => drop(job.id), 400);
  }
});

export function useJobs(): Job[] {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => jobs,
  );
}

/** Copy or move paths into a folder, in the background. Errors starting it show as a toast. */
export async function startTransfer(sources: string[], dest: string, mode: "copy" | "move") {
  try {
    await invoke<string>("job_transfer", { sources, dest, mode });
  } catch (err) {
    showToast(String(err), "error");
  }
}

export const cancelJob = (id: string) => void invoke("job_cancel", { id });
export const dismissJob = (id: string) => drop(id);
export const resolveJob = (id: string, choice: "replace" | "skip" | "keepBoth", all: boolean) =>
  void invoke("job_resolve", { id, choice, all }).catch((err) => showToast(String(err), "error"));
