import { useMemo, useState } from "react";

import { ThreatBadge } from "./Dashboard";
import { formatBytes, humanize, truncatePath } from "../lib/format";
import type {
  InstalledApp,
  PurgeOptions,
  ResidueItem,
  ResidueReport,
} from "../types";

const DEFAULT_OPTIONS: PurgeOptions = {
  run_official_uninstaller: true,
  create_restore_point: true,
  kill_processes: true,
  remove_services: true,
  remove_tasks: true,
  remove_startup: true,
  defer_to_reboot: true,
  remove_appx: true,
};

const OPTION_LABELS: { key: keyof PurgeOptions; label: string }[] = [
  { key: "run_official_uninstaller", label: "Execute vendor uninstaller" },
  { key: "create_restore_point", label: "Create system restore point" },
  { key: "kill_processes", label: "Terminate process trees" },
  { key: "remove_services", label: "Dissolve services and drivers" },
  { key: "remove_tasks", label: "Purge scheduled tasks" },
  { key: "remove_startup", label: "Strip autostart hooks" },
  { key: "defer_to_reboot", label: "Defer locked files to reboot" },
  { key: "remove_appx", label: "Deregister MSIX packages" },
];

interface ChamberProps {
  application: InstalledApp | null;
  report: ResidueReport | null;
  loading: boolean;
  onClose: () => void;
  onPurge: (application: InstalledApp, options: PurgeOptions) => void;
}

function ResidueRow({ item }: { item: ResidueItem }) {
  return (
    <div className={`residue-item ${item.protected ? "protected" : ""}`}>
      <div className="residue-kind">{humanize(item.kind)}</div>
      <div>
        <div className="residue-path">{truncatePath(item.location, 120)}</div>
        <div className="residue-detail">
          {item.protected
            ? `PROTECTED — ${item.protection_reason ?? "safety guard"}`
            : item.detail}
        </div>
      </div>
      <div className="residue-size">
        {item.size_bytes > 0 ? formatBytes(item.size_bytes) : "—"}
      </div>
    </div>
  );
}

export default function Chamber({
  application,
  report,
  loading,
  onClose,
  onPurge,
}: ChamberProps) {
  const [options, setOptions] = useState<PurgeOptions>(DEFAULT_OPTIONS);

  const grouped = useMemo(() => {
    const counts = new Map<string, number>();
    for (const item of report?.items ?? []) {
      counts.set(item.kind, (counts.get(item.kind) ?? 0) + 1);
    }
    return [...counts.entries()].sort((left, right) => right[1] - left[1]);
  }, [report]);

  if (!application) {
    return null;
  }

  const toggle = (key: keyof PurgeOptions) => {
    setOptions((current) => ({ ...current, [key]: !current[key] }));
  };

  return (
    <div className="chamber-backdrop" role="dialog" aria-modal="true">
      <div className="chamber">
        <div className="chamber-head">
          <div>
            <h2>Disintegration Chamber</h2>
            <div className="chamber-meta">
              <span>{application.publisher ?? "UNKNOWN PUBLISHER"}</span>
              <span>V{application.display_version ?? "—"}</span>
              <span>{application.architecture.toUpperCase()}</span>
              <span>{application.source.toUpperCase()}</span>
            </div>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            {report ? <ThreatBadge level={report.threat} /> : null}
            <button className="btn btn-ghost" onClick={onClose}>
              Abort
            </button>
          </div>
        </div>

        <div className="chamber-body">
          <div className="chamber-scan">
            {loading ? (
              <div className="empty-state">
                <span className="spinner" />
                <div>Sweeping the machine for hidden residue…</div>
              </div>
            ) : report && report.items.length > 0 ? (
              report.items.map((item) => <ResidueRow key={item.id} item={item} />)
            ) : (
              <div className="empty-state">
                <div className="glyph">◇</div>
                <div>No residue detected - the payload is already clean.</div>
              </div>
            )}
          </div>

          <aside className="chamber-side">
            <div className="side-block">
              <h3>Target Profile</h3>
              <div className="stat-row">
                <span>Install path</span>
                <span title={application.install_location ?? ""}>
                  {application.install_location
                    ? truncatePath(application.install_location, 26)
                    : "—"}
                </span>
              </div>
              <div className="stat-row">
                <span>Executable</span>
                <span title={application.executable ?? ""}>
                  {application.executable
                    ? truncatePath(application.executable, 26)
                    : "—"}
                </span>
              </div>
              <div className="stat-row">
                <span>Reported size</span>
                <span>{formatBytes(application.estimated_size_bytes)}</span>
              </div>
              <div className="stat-row">
                <span>Installed</span>
                <span>{application.install_date ?? "—"}</span>
              </div>
            </div>

            <div className="side-block">
              <h3>Threat Assessment</h3>
              <div className="stat-row">
                <span>Score</span>
                <span>{report?.score ?? 0} / 100</span>
              </div>
              <div className="stat-row">
                <span>Artefacts</span>
                <span>{report?.items.length ?? 0}</span>
              </div>
              <div className="stat-row">
                <span>Reclaimable</span>
                <span>{formatBytes(report?.total_bytes ?? 0)}</span>
              </div>
              <div className="stat-row">
                <span>Guarded</span>
                <span>{report?.protected_items ?? 0}</span>
              </div>
              {grouped.map(([kind, count]) => (
                <div className="stat-row" key={kind}>
                  <span>{humanize(kind)}</span>
                  <span>{count}</span>
                </div>
              ))}
            </div>

            <div className="side-block">
              <h3>Purge Protocol</h3>
              {OPTION_LABELS.map(({ key, label }) => (
                <div
                  className="toggle-row"
                  key={key}
                  onClick={() => toggle(key)}
                  role="switch"
                  aria-checked={options[key]}
                >
                  <span>{label}</span>
                  <span className={`toggle ${options[key] ? "on" : ""}`} />
                </div>
              ))}
            </div>

            {report && report.warnings.length > 0 ? (
              <div className="side-block">
                <h3>Advisories</h3>
                {report.warnings.map((warning) => (
                  <div className="residue-detail" key={warning}>
                    • {warning}
                  </div>
                ))}
              </div>
            ) : null}
          </aside>
        </div>

        <div className="chamber-foot">
          <div className="label">
            {report
              ? `${report.items.length} artefact(s) queued for dissolution`
              : "Awaiting scan"}
          </div>
          <div style={{ display: "flex", gap: 12 }}>
            <button className="btn btn-ghost" onClick={onClose}>
              Cancel
            </button>
            <button
              className="btn btn-primary"
              disabled={loading}
              onClick={() => onPurge(application, options)}
            >
              Engage Disintegration
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
