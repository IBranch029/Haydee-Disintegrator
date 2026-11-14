import { useMemo, useState } from "react";

import { formatBytes } from "../lib/format";
import type { InstalledApp, ThreatLevel } from "../types";

const THREAT_LABELS: Record<ThreatLevel, string> = {
  minimal: "MINIMAL",
  low: "LOW",
  elevated: "ELEVATED",
  high: "HIGH",
  critical: "CRITICAL",
};

export function ThreatBadge({ level }: { level: ThreatLevel }) {
  return (
    <span className={`threat threat-${level}`}>
      <span className="pip" />
      {THREAT_LABELS[level]}
    </span>
  );
}

interface DashboardProps {
  applications: InstalledApp[];
  loading: boolean;
  selectedId: string | null;
  onSelect: (application: InstalledApp) => void;
  onRefresh: () => void;
}

type SortKey = "name" | "size" | "date";

export default function Dashboard({
  applications,
  loading,
  selectedId,
  onSelect,
  onRefresh,
}: DashboardProps) {
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<SortKey>("name");

  const rows = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = needle
      ? applications.filter(
          (app) =>
            app.display_name.toLowerCase().includes(needle) ||
            (app.publisher ?? "").toLowerCase().includes(needle) ||
            (app.display_version ?? "").toLowerCase().includes(needle),
        )
      : applications;

    const sorted = [...filtered];
    if (sort === "size") {
      sorted.sort(
        (left, right) =>
          (right.estimated_size_bytes ?? 0) - (left.estimated_size_bytes ?? 0),
      );
    } else if (sort === "date") {
      sorted.sort((left, right) =>
        (right.install_date ?? "").localeCompare(left.install_date ?? ""),
      );
    } else {
      sorted.sort((left, right) =>
        left.display_name.localeCompare(right.display_name),
      );
    }
    return sorted;
  }, [applications, query, sort]);

  return (
    <section className="panel">
      <div className="panel-accent" />
      <div className="panel-head">
        <div className="panel-title">
          Target Inventory
          <span className="label">{rows.length} detected</span>
        </div>
        <button className="btn btn-ghost" onClick={onRefresh} disabled={loading}>
          {loading ? <span className="spinner" /> : null}
          Rescan System
        </button>
      </div>

      <div className="toolbar">
        <div className="search">
          <input
            value={query}
            placeholder="FILTER BY NAME, PUBLISHER OR VERSION"
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>

        <div className="segmented" role="group" aria-label="Sort targets">
          {(
            [
              ["name", "Name"],
              ["size", "Size"],
              ["date", "Install Date"],
            ] as [SortKey, string][]
          ).map(([key, label]) => (
            <button
              key={key}
              className={sort === key ? "active" : ""}
              onClick={() => setSort(key)}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div className="panel-body">
        {rows.length === 0 ? (
          <div className="empty-state">
            <div className="glyph">◇</div>
            <div>
              {loading
                ? "Sweeping the machine for installed payloads…"
                : "No target matches the current filter."}
            </div>
          </div>
        ) : (
          <table className="target-table">
            <thead>
              <tr>
                <th>Payload</th>
                <th>Publisher</th>
                <th>Version</th>
                <th>Size</th>
                <th>Installed</th>
                <th>Arch</th>
                <th>Source</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((application) => (
                <tr
                  key={application.id}
                  className={`target-row ${
                    application.id === selectedId ? "selected" : ""
                  }`}
                  onClick={() => onSelect(application)}
                >
                  <td>
                    <div className="app-name">
                      <strong>{application.display_name}</strong>
                      <small>
                        {application.install_location ?? application.registry_key ?? "—"}
                      </small>
                    </div>
                  </td>
                  <td className="mono" style={{ fontSize: 11 }}>
                    {application.publisher ?? "—"}
                  </td>
                  <td className="mono" style={{ fontSize: 11 }}>
                    {application.display_version ?? "—"}
                  </td>
                  <td className="mono" style={{ fontSize: 11 }}>
                    {formatBytes(application.estimated_size_bytes)}
                  </td>
                  <td className="mono" style={{ fontSize: 11 }}>
                    {application.install_date ?? "—"}
                  </td>
                  <td>
                    <span className="tag">{application.architecture}</span>
                  </td>
                  <td>
                    <span
                      className={`tag ${
                        application.source === "appx" ? "hazard" : ""
                      }`}
                    >
                      {application.source === "appx" ? "APPX" : "REGISTRY"}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}
