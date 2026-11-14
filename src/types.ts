/**
 * Data contract mirroring the Rust engine (`src-tauri/src/model.rs`).
 * Every field name matches the serde representation of the Rust structures.
 */

export type AppSource = "registry" | "appx";

export type ThreatLevel =
  | "minimal"
  | "low"
  | "elevated"
  | "high"
  | "critical";

export type ResidueKind =
  | "file"
  | "directory"
  | "registry_key"
  | "registry_value"
  | "service"
  | "scheduled_task"
  | "startup_entry"
  | "process"
  | "shell_extension";

export interface InstalledApp {
  id: string;
  display_name: string;
  display_version: string | null;
  publisher: string | null;
  install_date: string | null;
  install_location: string | null;
  uninstall_string: string | null;
  quiet_uninstall_string: string | null;
  estimated_size_bytes: number | null;
  system_component: boolean;
  windows_installer: boolean;
  parent_key_name: string | null;
  source: AppSource;
  registry_key: string | null;
  executable: string | null;
  architecture: string;
  tokens: string[];
}

export interface ResidueItem {
  id: string;
  kind: ResidueKind;
  location: string;
  detail: string;
  size_bytes: number;
  protected: boolean;
  protection_reason: string | null;
}

export interface ResidueReport {
  app: InstalledApp;
  threat: ThreatLevel;
  score: number;
  items: ResidueItem[];
  total_bytes: number;
  protected_items: number;
  warnings: string[];
}

export interface PurgeOptions {
  run_official_uninstaller: boolean;
  create_restore_point: boolean;
  kill_processes: boolean;
  remove_services: boolean;
  remove_tasks: boolean;
  remove_startup: boolean;
  defer_to_reboot: boolean;
  remove_appx: boolean;
}

export type PurgeStage =
  | "preflight"
  | "restore_point"
  | "terminate"
  | "uninstall"
  | "services"
  | "tasks"
  | "startup"
  | "files"
  | "registry"
  | "verify"
  | "complete"
  | "aborted";

export type EventSeverity = "info" | "warning" | "critical" | "success";

export interface PurgeEvent {
  stage: PurgeStage;
  severity: EventSeverity;
  message: string;
  bytes_delta: number;
  bytes_total: number;
  items_removed: number;
  progress: number;
}

export interface PurgeFailure {
  category: string;
  target: string;
  reason: string;
  deferred: boolean;
}

export interface PurgeReport {
  app_id: string;
  display_name: string;
  bytes_reclaimed: number;
  files_removed: number;
  files_deferred: number;
  registry_keys_removed: number;
  registry_values_removed: number;
  services_removed: number;
  tasks_removed: number;
  startup_removed: number;
  processes_terminated: number;
  protected_items: number;
  reboot_required: boolean;
  restore_point_created: boolean;
  duration_ms: number;
  failures: PurgeFailure[];
  warnings: string[];
}

export interface SystemTelemetry {
  cpu_percent: number;
  memory_total_bytes: number;
  memory_used_bytes: number;
  memory_percent: number;
  disk_total_bytes: number;
  disk_free_bytes: number;
  process_count: number;
  uptime_seconds: number;
}

export interface SystemInfo {
  os_name: string;
  os_version: string;
  build: string;
  architecture: string;
  is_elevated: boolean;
  user_name: string;
  machine_name: string;
  webview2_version: string | null;
  app_version: string;
}

export interface RestorePointInfo {
  sequence_number: number;
  description: string;
  created: boolean;
  detail: string;
}

export interface ScanFilter {
  query?: string | null;
  sort?: string | null;
  include_system_components?: boolean;
}
