import { useState } from "react";
import { cancelJob, dismissJob, resolveJob, useJobs, type Job } from "../lib/jobs";
import { formatDate, formatSize } from "../lib/format";
import { IS_MAC } from "../lib/platform";
import { Dialog } from "../shell/Dialog";
import { CloseSmallIcon } from "../shell/Icons";

/** Running copies and moves, bottom right; and the question when a name is taken. */
export function JobsPanel() {
  const jobs = useJobs();
  const asking = jobs.find((j) => j.state === "conflict" && j.conflict);
  const shown = jobs.filter((j) => j.state !== "done" && j.state !== "cancelled");
  return (
    <>
      {shown.length > 0 && (
        <div className="jobs" role="status" aria-live="polite">
          {shown.map((j) => (
            <JobCard key={j.id} job={j} />
          ))}
        </div>
      )}
      {asking && <ConflictDialog key={asking.id + asking.conflict!.name} job={asking} />}
    </>
  );
}

function JobCard({ job }: { job: Job }) {
  const p = job.progress;
  const pct = p.totalBytes > 0 ? Math.min(100, (p.doneBytes / p.totalBytes) * 100) : p.totalItems > 0 ? (p.doneItems / p.totalItems) * 100 : 0;
  const verb = job.kind === "copy" ? "Copying" : "Moving";
  if (job.state === "failed") {
    return (
      <div className="job failed">
        <div className="job-head">
          <span className="job-title">{job.kind === "copy" ? "Copy" : "Move"} failed</span>
          <button className="icon-btn small" title="Dismiss" onClick={() => dismissJob(job.id)}>
            <CloseSmallIcon size={12} />
          </button>
        </div>
        <div className="job-error">{job.message}</div>
      </div>
    );
  }
  return (
    <div className="job">
      <div className="job-head">
        <span className="job-title">
          {verb} {job.what} to “{job.dest}”
        </span>
        <button className="link-btn" onClick={() => cancelJob(job.id)}>
          Cancel
        </button>
      </div>
      <div className={`job-bar${p.counting ? " counting" : ""}`} role="progressbar" aria-valuenow={Math.round(pct)} aria-valuemin={0} aria-valuemax={100}>
        <span style={{ width: `${pct}%` }} />
      </div>
      <div className="job-detail">
        <span className="job-current">
          {job.state === "conflict"
            ? "Waiting for your answer…"
            : p.counting
              ? `Counting… ${p.totalItems.toLocaleString()} items, ${formatSize(p.totalBytes)}`
              : p.current}
        </span>
        {!p.counting && (
          <span className="mono">
            {p.doneItems.toLocaleString()} / {p.totalItems.toLocaleString()} items
            {p.totalBytes > 0 && ` · ${formatSize(p.doneBytes)} / ${formatSize(p.totalBytes)}`}
          </span>
        )}
      </div>
    </div>
  );
}

/** "a.txt already exists in src": Replace, Skip or Keep both, for this one or all. */
function ConflictDialog({ job }: { job: Job }) {
  const c = job.conflict!;
  const [all, setAll] = useState(false);
  const answer = (choice: "replace" | "skip" | "keepBoth") => resolveJob(job.id, choice, all);
  const row = (label: string, size: number, modified: number) => (
    <tr>
      <th>{label}</th>
      <td className="mono">{c.isFolder ? "Folder" : formatSize(size)}</td>
      <td className="mono">{formatDate(modified)}</td>
    </tr>
  );
  return (
    <Dialog
      title={`“${c.name}” already exists in “${c.folder}”`}
      onClose={() => cancelJob(job.id)}
      width={520}
      footer={
        <>
          <label className="check">
            <input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} />
            Do this for the rest
          </label>
          <span className="spacer" />
          <button className="btn" onClick={() => answer("skip")}>
            Skip
          </button>
          <button className="btn" onClick={() => answer("keepBoth")}>
            Keep both
          </button>
          <button className="btn primary" onClick={() => answer("replace")}>
            Replace
          </button>
        </>
      }
    >
      <p>
        {job.kind === "copy" ? "Copying" : "Moving"} {job.what}. Replacing sends the existing one to{" "}
        {IS_MAC ? "the Trash" : "the Recycle Bin"}; keeping both gives the new one a free name.
      </p>
      <table className="kv conflict-table">
        <tbody>
          {row("Existing", c.targetSize, c.targetModified)}
          {row("New", c.sourceSize, c.sourceModified)}
        </tbody>
      </table>
    </Dialog>
  );
}
