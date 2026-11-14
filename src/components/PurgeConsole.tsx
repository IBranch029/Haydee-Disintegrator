import { useEffect, useRef } from "react";

import { formatBytes, formatDuration, humanize } from "../lib/format";
import type { InstalledApp, PurgeEvent, PurgeReport } from "../types";

interface PurgeConsoleProps {
  application: InstalledApp;
  events: PurgeEvent[];
  report: PurgeReport | null;
  running: boolean;
  onClose: () => void;
  onExport: () => void;
}

function ProgressRing({ progress }: { progress: number }) {
  const radius = 30;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference * (1 - Math.min(1, Math.max(0, progress)));

  return (
    <div className="ring">
      <svg width="74" height="74" viewBox="0 0 74 74">
        <circle
          className="ring-track"
          cx="37"
          cy="37"
          r={radius}
          fill="none"
          strokeWidth="6"
        />
        <circle
          className="ring-progress"
          cx="37"
          cy="37"
          r={radius}
          fill="none"
          strokeWidth="6"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
        />
      </svg>
      <div className="ring-label">{Math.round(progress * 100)}%</div>
    </div>
  );
}

export default function PurgeConsole({
  application,
  events,
  report,
  running,
  onClose,
  onExport,
}: PurgeConsoleProps) {
  const logRef = useRef<HTMLDivElement | null>(null);
  const lastEvent = events[events.length - 1];
  const progress = lastEvent?.progress ?? (running ? 0.02 : 1);
  const bytes = report?.bytes_reclaimed ?? lastEvent?.bytes_total ?? 0;
  const items =
    report?.files_removed ??
    report?.registry_keys_removed ??
    lastEvent?.items_removed ??
    0;

  useEffect(() => {
    const element = logRef.current;
    if (element) {
      element.scrollTop = element.scrollHeight;
    }
  }, [events]);

  return (
    <div className="console-backdrop" role="dialog" aria-modal="true">
      <div className="console">
        <div className="console-head">
          <div>
            <h2>Disintegration Sequence</h2>
            <div className="chamber-meta">
              <span>TARGET // {application.display_name}</span>
              <span>
                {running ? "PROTOCOL ACTIVE" : "PROTOCOL COMPLETE"}
              </span>
            </div>
          </div>
          <div className="ring-wrap">
            <ProgressRing progress={progress} />
          </div>
        </div>

        <div className="console-stats">
          <div className="console-stat">
            <span className="label">Reclaimed</span>
            <strong>{formatBytes(bytes)}</strong>
          </div>
          <div className="console-stat">
            <span className="label">Artefacts</span>
            <strong>{items}</strong>
          </div>
          <div className="console-stat">
            <span className="label">Duration</span>
            <strong>
              {report ? formatDuration(report.duration_ms) : "—"}
            </strong>
          </div>
          <div className="console-stat">
            <span className="label">Failures</span>
            <strong>{report?.failures.length ?? 0}</strong>
          </div>
        </div>

        <div className="console-log" ref={logRef}>
          {events.map((event, index) => (
            <div className={`log-line ${event.severity}`} key={`${event.stage}-${index}`}>
              <span className="stage">{humanize(event.stage)}</span>
              <span className="message">{event.message}</span>
              <span className="stamp">{Math.round(event.progress * 100)}%</span>
            </div>
          ))}
          {events.length === 0 ? (
            <div className="log-line">
              <span className="stage">standby</span>
              <span className="message">Awaiting engagement…</span>
              <span className="stamp">0%</span>
            </div>
          ) : null}
        </div>

        <div className="console-foot">
          <div className="label">
            {report
              ? `Finished in ${formatDuration(report.duration_ms)} · ${
                  report.reboot_required
                    ? "reboot required for deferred deletions"
                    : "no reboot required"
                }`
              : "Executing purge protocol…"}
          </div>
          <div style={{ display: "flex", gap: 12 }}>
            <button
              className="btn btn-ghost"
              onClick={onExport}
              disabled={!report}
            >
              Export Report
            </button>
            <button className="btn btn-primary" onClick={onClose} disabled={running}>
              {running ? "Standby" : "Close Console"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
