/**
 * Typed bridge to the Rust engine.
 *
 * Every call goes through the Tauri IPC layer; the `Channel` is used for the
 * realtime event stream of the purge pipeline.
 */

import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  InstalledApp,
  PurgeEvent,
  PurgeOptions,
  PurgeReport,
  ResidueReport,
  RestorePointInfo,
  ScanFilter,
  SystemInfo,
  SystemTelemetry,
} from "../types";

/** Static information about the host. */
export function fetchSystemInfo(): Promise<SystemInfo> {
  return invoke<SystemInfo>("system_info");
}

/** Realtime telemetry sample. */
export function fetchTelemetry(): Promise<SystemTelemetry> {
  return invoke<SystemTelemetry>("telemetry");
}

/** Installed application inventory. */
export function fetchApplications(
  filter?: ScanFilter,
): Promise<InstalledApp[]> {
  return invoke<InstalledApp[]>("scan_applications", { filter: filter ?? null });
}

/** Deep inspection of a single target. */
export function analyzeApplication(appId: string): Promise<ResidueReport> {
  return invoke<ResidueReport>("analyze_application", { appId });
}

/**
 * Executes the purge pipeline.
 *
 * `onEvent` receives every realtime event emitted by the engine.
 */
export async function purgeApplication(
  appId: string,
  options: PurgeOptions,
  onEvent: (event: PurgeEvent) => void,
): Promise<PurgeReport> {
  const channel = new Channel<PurgeEvent>();
  channel.onmessage = onEvent;

  return invoke<PurgeReport>("purge_application", {
    appId,
    options,
    onEvent: channel,
  });
}

/** Creates a system restore point. */
export function createRestorePoint(
  description?: string,
): Promise<RestorePointInfo> {
  return invoke<RestorePointInfo>("create_restore_point", {
    description: description ?? null,
  });
}

/** Writes a purge report as a JSON document and returns its path. */
export function exportReport(
  appId: string,
  report: PurgeReport,
): Promise<string> {
  return invoke<string>("export_report", { appId, report });
}

/** Re-scans the registry for surviving residue. */
export function rescanRegistry(appId: string): Promise<string[]> {
  return invoke<string[]>("rescan_registry", { appId });
}
