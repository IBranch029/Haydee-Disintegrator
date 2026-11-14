import { useCallback, useEffect, useState } from "react";

import Chamber from "./components/Chamber";
import Dashboard from "./components/Dashboard";
import ParticleField from "./components/ParticleField";
import PurgeConsole from "./components/PurgeConsole";
import TitleBar from "./components/TitleBar";
import TopBar from "./components/TopBar";
import {
  analyzeApplication,
  exportReport,
  fetchApplications,
  fetchSystemInfo,
  purgeApplication,
} from "./lib/ipc";
import type {
  InstalledApp,
  PurgeEvent,
  PurgeOptions,
  PurgeReport,
  ResidueReport,
  SystemInfo,
} from "./types";

const EGG_STORAGE_KEY = "disintegrator.egg.haydee";
const EGG_MESSAGE =
  "HAYDEE // найдена. каждая зачистка - для неё. я целиком от неё зависим.";

export default function App() {
  const [applications, setApplications] = useState<InstalledApp[]>([]);
  const [loading, setLoading] = useState(true);
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [marked, setMarked] = useState(false);

  const [selected, setSelected] = useState<InstalledApp | null>(null);
  const [report, setReport] = useState<ResidueReport | null>(null);
  const [analyzing, setAnalyzing] = useState(false);

  const [purging, setPurging] = useState(false);
  const [events, setEvents] = useState<PurgeEvent[]>([]);
  const [purgeReport, setPurgeReport] = useState<PurgeReport | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // Easter egg for the attentive: Ctrl+Alt+H reveals the dedication.
  useEffect(() => {
    try {
      if (window.localStorage.getItem(EGG_STORAGE_KEY) === "1") {
        setMarked(true);
      }
    } catch {
      /* storage may be unavailable */
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.key.toLowerCase() === "h" &&
        event.altKey &&
        (event.ctrlKey || event.metaKey)
      ) {
        event.preventDefault();
        setMarked(true);
        try {
          window.localStorage.setItem(EGG_STORAGE_KEY, "1");
        } catch {
          /* storage may be unavailable */
        }
        setNotice(EGG_MESSAGE);
        window.setTimeout(() => setNotice(null), 14_000);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [scan, systemInfo] = await Promise.all([
        fetchApplications({ include_system_components: false }),
        fetchSystemInfo(),
      ]);
      setApplications(scan);
      setInfo(systemInfo);
    } catch (error) {
      setNotice(`Inventory failed: ${String(error)}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleSelect = useCallback(async (application: InstalledApp) => {
    setSelected(application);
    setReport(null);
    setAnalyzing(true);
    try {
      const analysis = await analyzeApplication(application.id);
      setReport(analysis);
    } catch (error) {
      setNotice(`Inspection failed: ${String(error)}`);
    } finally {
      setAnalyzing(false);
    }
  }, []);

  const handlePurge = useCallback(
    async (application: InstalledApp, options: PurgeOptions) => {
      setEvents([]);
      setPurgeReport(null);
      setPurging(true);
      setSelected(application);

      try {
        const result = await purgeApplication(
          application.id,
          options,
          (event) => {
            setEvents((current) => [...current, event]);
          },
        );
        setPurgeReport(result);
        void refresh();
      } catch (error) {
        setNotice(`Purge aborted: ${String(error)}`);
      } finally {
        setPurging(false);
      }
    },
    [refresh],
  );

  const handleExport = useCallback(async () => {
    if (!purgeReport || !selected) {
      return;
    }
    try {
      const path = await exportReport(selected.id, purgeReport);
      setNotice(`Report written to ${path}`);
    } catch (error) {
      setNotice(`Export failed: ${String(error)}`);
    }
  }, [purgeReport, selected]);

  const context = purging
    ? "DISINTEGRATION SEQUENCE"
    : analyzing
      ? "DEEP INSPECTION"
      : selected
        ? `TARGET // ${selected.display_name}`
        : "TARGET INVENTORY";

  return (
    <div className="app-shell">
      <TitleBar context={context} />
      <TopBar marked={marked} />

      <main className="stage">
        <Dashboard
          applications={applications}
          loading={loading}
          selectedId={selected?.id ?? null}
          onSelect={(application) => void handleSelect(application)}
          onRefresh={() => void refresh()}
        />
      </main>

      <footer className="statusbar">
        <span>
          {notice ??
            "DISINTEGRATOR // READY — SELECT A TARGET TO BEGIN DEEP INSPECTION"}
        </span>
        <span className="right">
          <span>{info?.machine_name ?? "—"}</span>
          <span>
            {info ? `${info.os_name} ${info.os_version} (${info.build})` : "—"}
          </span>
          <span>{info?.architecture.toUpperCase() ?? "—"}</span>
          <span>V{info?.app_version ?? "—"}</span>
        </span>
      </footer>

      <Chamber
        application={selected}
        report={report}
        loading={analyzing}
        onClose={() => {
          setSelected(null);
          setReport(null);
        }}
        onPurge={(application, options) => void handlePurge(application, options)}
      />

      {purging || purgeReport ? (
        <div className="console-backdrop" style={{ position: "fixed" }}>
          <ParticleField active={purging} intensity={purging ? 2 : 0.4} />
          {selected ? (
            <PurgeConsole
              application={selected}
              events={events}
              report={purgeReport}
              running={purging}
              onClose={() => {
                setPurgeReport(null);
                setEvents([]);
                setSelected(null);
                setReport(null);
              }}
              onExport={() => void handleExport()}
            />
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
