import { useEffect, useState } from "react";

import { fetchSystemInfo, fetchTelemetry } from "../lib/ipc";
import { clampPercent, formatBytes, formatUptime } from "../lib/format";
import { SparkIcon } from "./icons";
import type { SystemInfo, SystemTelemetry } from "../types";

interface GaugeProps {
  label: string;
  value: string;
  percent: number;
  tone?: "hazard" | "emerald" | "violet";
}

function Gauge({ label, value, percent, tone = "hazard" }: GaugeProps) {
  const width = clampPercent(percent);
  return (
    <div className="gauge">
      <div className="gauge-head">
        <span className="label">{label}</span>
        <span className="gauge-value">{value}</span>
      </div>
      <div className="gauge-track">
        <div
          className={`gauge-fill ${tone === "hazard" ? "" : tone}`}
          style={{ width: `${width}%` }}
        />
      </div>
    </div>
  );
}

/** The wordmark is split into glyphs so that it can span the whole header. */
const WORDMARK = ["H", "A", "Y", "D", "E", "E"];

export default function TopBar({ marked = false }: { marked?: boolean }) {
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [telemetry, setTelemetry] = useState<SystemTelemetry | null>(null);

  useEffect(() => {
    let cancelled = false;

    fetchSystemInfo()
      .then((value) => {
        if (!cancelled) {
          setInfo(value);
        }
      })
      .catch(() => undefined);

    const timer = window.setInterval(() => {
      fetchTelemetry()
        .then((value) => {
          if (!cancelled) {
            setTelemetry(value);
          }
        })
        .catch(() => undefined);
    }, 1000);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, []);

  return (
    <header className="topbar">
      <div className="brand">
        <div className="brand-mark">
          <span>H</span>
        </div>

        <div className="brand-text">
          <div className="wordmark" title="HAYDEE">
            {WORDMARK.map((glyph, index) => (
              <span className="wordmark-glyph" key={`${glyph}-${index}`}>
                {glyph}
              </span>
            ))}
            {marked ? (
              <span className="wordmark-spark" title="found">
                <SparkIcon />
              </span>
            ) : null}
          </div>
          <small>DEEP PURGE CONSOLE // DISINTEGRATOR</small>
        </div>
      </div>

      <div className="telemetry-strip">
        <Gauge
          label="CPU LOAD"
          value={telemetry ? `${telemetry.cpu_percent.toFixed(1)}%` : "—"}
          percent={telemetry?.cpu_percent ?? 0}
        />
        <Gauge
          label="MEMORY"
          value={
            telemetry
              ? `${formatBytes(telemetry.memory_used_bytes)} / ${formatBytes(
                  telemetry.memory_total_bytes,
                )}`
              : "—"
          }
          percent={telemetry?.memory_percent ?? 0}
          tone="violet"
        />
        <Gauge
          label="SYSTEM VOLUME"
          value={
            telemetry ? `${formatBytes(telemetry.disk_free_bytes)} FREE` : "—"
          }
          percent={
            telemetry && telemetry.disk_total_bytes > 0
              ? (telemetry.disk_free_bytes / telemetry.disk_total_bytes) * 100
              : 0
          }
          tone="emerald"
        />
        <Gauge
          label="PROCESSES"
          value={telemetry ? `${telemetry.process_count}` : "—"}
          percent={
            telemetry && telemetry.uptime_seconds > 0
              ? Math.min(100, (telemetry.process_count / 400) * 100)
              : 0
          }
        />
      </div>

      <div className="topbar-right">
        <div className="elevation-chip">
          <span className="dot" />
          UPTIME {telemetry ? formatUptime(telemetry.uptime_seconds) : "—"}
        </div>
        <div
          className={`elevation-chip ${
            info?.is_elevated ? "active" : "inactive"
          }`}
        >
          <span className="dot" />
          {info?.is_elevated ? "ELEVATED" : "NOT ELEVATED"}
        </div>
      </div>
    </header>
  );
}
