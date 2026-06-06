import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface LauncherConfig {
  install_dir: string;
  manifest_url: string;
  api_base: string;
  server_host: string;
  server_port: number;
  concurrency: number;
}

export interface CheckResult {
  version: string;
  needs_update: boolean;
  missing: number;
  mismatched: number;
  bytes_to_download: number;
  files_total: number;
}

export interface PlayResult {
  launched: boolean;
  bad: string[];
}

export interface ScanSummary {
  ok: number;
  missing: number;
  mismatched: number;
  bytes_to_download: number;
  checked: number;
}

export interface Progress {
  downloaded: number;
  total: number;
  files_done: number;
  files_total: number;
  speed_bps: number;
  eta_secs: number;
  current: string;
  done: boolean;
}

export const api = {
  getConfig: () => invoke<LauncherConfig>("get_config"),
  saveConfig: (config: LauncherConfig) => invoke<void>("save_config", { config }),
  checkUpdate: () => invoke<CheckResult>("check_update"),
  startUpdate: () => invoke<void>("start_update"),
  repair: () => invoke<ScanSummary>("repair"),
  verifyFiles: () => invoke<ScanSummary>("verify_files"),
  play: () => invoke<PlayResult>("play"),
};

export function onProgress(cb: (p: Progress) => void): Promise<UnlistenFn> {
  return listen<Progress>("update:progress", (e) => cb(e.payload));
}

// --- форматтеры ---

export function fmtBytes(n: number): string {
  if (n <= 0) return "0 Б";
  const units = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  return `${(n / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

export function fmtSpeed(bps: number): string {
  return `${fmtBytes(bps)}/с`;
}

export function fmtEta(secs: number): string {
  if (secs <= 0) return "—";
  if (secs < 60) return `${secs} с`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  if (m < 60) return `${m} мин ${s} с`;
  const h = Math.floor(m / 60);
  return `${h} ч ${m % 60} мин`;
}
