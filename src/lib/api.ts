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
  cancelled: boolean;
}

export interface Progress {
  phase: "download" | "verify";
  processed: number;
  total: number;
  files_done: number;
  files_total: number;
  speed_bps: number;
  eta_secs: number;
  current: string;
  paused: boolean;
  done: boolean;
}

export interface ServerInfo {
  id: string;
  name: string;
  online: boolean;
  players: number;
  max: number;
  started_at: number;
}

export interface SelfUpdateInfo {
  /** Доступная новая версия лаунчера. */
  version: string;
  /** Текущая (установленная) версия. */
  current: string;
}

export interface ClientSettings {
  /** Режим производительности включён (существует system/d3d8.dll). */
  performance: boolean;
  /** Язык клиента: "ru" | "en". */
  language: string;
}

/** Состояние Smart App Control (Windows 11). */
export type SacState = "off" | "on" | "evaluation" | "unknown";

/** Игрок, вошедший в лаунчер (OAuth сайта). */
export interface LauncherUser {
  id: string;
  name: string | null;
  email: string | null;
  image: string | null;
}

/** Начало OAuth-входа: код + URL подтверждения + приватный секрет поллинга. */
export interface BeginAuth {
  code: string;
  secret: string;
  verify_url: string;
  expires_in: number;
}

export interface PollResult {
  status: "pending" | "approved" | "expired";
  token?: string | null;
}

export interface GameAccount {
  login: string;
}

export interface ReportResult {
  ticket_id: number;
  attached: number;
  rejected: number;
}

/** Сводка состояния защиты и целостности для панели «Состояние». */
export interface Diagnostics {
  launcher_version: string;
  client_version: string;
  /** true — подпись валидна; null — не удалось проверить (оффлайн). */
  manifest_signature_ok: boolean | null;
  exe_present: boolean;
  sac: SacState;
  defender_excluded: boolean;
  install_dir: string;
}

export const api = {
  getConfig: () => invoke<LauncherConfig>("get_config"),
  saveConfig: (config: LauncherConfig) => invoke<void>("save_config", { config }),
  serverStatus: () => invoke<ServerInfo[]>("server_status"),
  checkUpdate: () => invoke<CheckResult>("check_update"),
  startUpdate: () => invoke<void>("start_update"),
  repair: () => invoke<ScanSummary>("repair"),
  verifyFiles: () => invoke<ScanSummary>("verify_files"),
  play: () => invoke<PlayResult>("play"),
  pause: () => invoke<void>("pause_tasks"),
  resume: () => invoke<void>("resume_tasks"),
  cancel: () => invoke<void>("cancel_tasks"),
  checkSelfUpdate: () => invoke<SelfUpdateInfo | null>("check_self_update"),
  applySelfUpdate: () => invoke<void>("apply_self_update"),
  sacStatus: () => invoke<SacState>("sac_status"),
  openSacSettings: () => invoke<void>("open_sac_settings"),
  diagnostics: () => invoke<Diagnostics>("diagnostics"),
  authBegin: () => invoke<BeginAuth>("auth_begin"),
  authPoll: (secret: string) => invoke<PollResult>("auth_poll", { secret }),
  authLogout: () => invoke<void>("auth_logout"),
  authMe: () => invoke<LauncherUser | null>("auth_me"),
  listGameAccounts: () => invoke<GameAccount[]>("list_game_accounts"),
  createGameAccount: (login: string, password: string) =>
    invoke<string>("create_game_account", { login, password }),
  claimGameAccount: (login: string, password: string) =>
    invoke<string>("claim_game_account", { login, password }),
  changeGameAccountPassword: (login: string, password: string) =>
    invoke<void>("change_game_account_password", { login, password }),
  submitBugReport: (
    category: string,
    subcategory: string,
    title: string,
    description: string,
    files: string[],
  ) => invoke<ReportResult>("submit_bug_report", { category, subcategory, title, description, files }),
  getClientSettings: () => invoke<ClientSettings>("get_client_settings"),
  setPerformanceMode: (enabled: boolean) => invoke<void>("set_performance_mode", { enabled }),
  setClientLanguage: (lang: string) => invoke<void>("set_client_language", { lang }),
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

export function fmtUptime(startedAt: number, nowSec: number): string {
  const s = Math.max(0, nowSec - startedAt);
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return `${d}д ${h}ч ${m}м`;
  if (h > 0) return `${h}ч ${m}м`;
  return `${m}м`;
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
