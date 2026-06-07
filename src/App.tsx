import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { check as checkLauncherUpdate } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  Play,
  ShieldCheck,
  RefreshCw,
  Settings as SettingsIcon,
  Download,
  FolderOpen,
  AlertTriangle,
  CheckCircle2,
  Pause,
  X,
  Square,
} from "lucide-react";
import {
  api,
  onProgress,
  fmtBytes,
  fmtSpeed,
  fmtEta,
  type LauncherConfig,
  type Progress,
  type ServerInfo,
} from "./lib/api";
import { TitleBar } from "./components/TitleBar";
import { Sigil } from "./components/Sigil";
import { Ambient } from "./components/Ambient";

type Phase =
  | "checking"
  | "outdated"
  | "updating"
  | "ready"
  | "repairing"
  | "verifying"
  | "playing"
  | "error";

const RATES = [
  { label: "EXP", value: "x1" },
  { label: "SP", value: "x1" },
  { label: "Adena", value: "x1" },
  { label: "Drop", value: "x1" },
];


// Операции с прогрессом, которые можно ставить на паузу/отменять.
const RUNNING: Phase[] = ["updating", "repairing", "verifying"];

export default function App() {
  const [phase, setPhase] = useState<Phase>("checking");
  const [status, setStatus] = useState("Проверка обновлений…");
  const [version, setVersion] = useState("—");
  const [bytesNeeded, setBytesNeeded] = useState(0);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [paused, setPaused] = useState(false);
  const [bad, setBad] = useState<string[]>([]);
  const [config, setConfig] = useState<LauncherConfig | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [srv, setSrv] = useState<ServerInfo[] | null>(null);
  const unlisten = useRef<(() => void) | null>(null);

  useEffect(() => {
    (async () => {
      unlisten.current = await onProgress((p) => {
        setProgress(p);
        setPaused(p.paused);
      });
      try {
        setConfig(await api.getConfig());
      } catch {
        /* ignore */
      }
      await selfUpdate();
      await runCheck();
    })();
    return () => unlisten.current?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Живой статус сервера: опрос раз в 30с.
  useEffect(() => {
    let alive = true;
    const load = () =>
      api
        .serverStatus()
        .then((s) => alive && setSrv(s))
        .catch(() => {});
    load();
    const id = setInterval(load, 30_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  // Самообновление лаунчера из GitHub-релизов (Tauri updater). Тихо игнорируем
  // ошибки (оффлайн/dev) — тогда просто продолжаем со старой версией.
  async function selfUpdate() {
    try {
      setStatus("Проверка обновлений лаунчера…");
      const upd = await checkLauncherUpdate();
      if (upd) {
        setStatus(`Обновление лаунчера до ${upd.version}…`);
        await upd.downloadAndInstall();
        await relaunch();
      }
    } catch {
      /* нет обновления / оффлайн / dev — продолжаем */
    }
  }

  async function runCheck() {
    setPhase("checking");
    setStatus("Проверка обновлений…");
    try {
      const r = await api.checkUpdate();
      setVersion(r.version);
      setBytesNeeded(r.bytes_to_download);
      if (r.needs_update) {
        setPhase("outdated");
        setStatus(`Доступно обновление · ${fmtBytes(r.bytes_to_download)}`);
      } else {
        setPhase("ready");
        setStatus("Клиент в актуальном состоянии");
      }
    } catch (e) {
      setPhase("error");
      setStatus(`Ошибка проверки: ${e}`);
    }
  }

  async function runUpdate() {
    setProgress(null);
    setPaused(false);
    setPhase("updating");
    setStatus("Загрузка обновления…");
    try {
      await api.startUpdate();
      await runCheck();
    } catch (e) {
      setPhase("error");
      setStatus(`Ошибка загрузки: ${e}`);
    }
  }

  async function runVerify() {
    setProgress(null);
    setPaused(false);
    setPhase("verifying");
    setStatus("Проверка целостности файлов…");
    try {
      const s = await api.verifyFiles();
      if (s.cancelled) {
        setStatus("Проверка отменена");
      } else if (s.missing + s.mismatched === 0) {
        setStatus(`Все файлы целы (${s.ok})`);
      } else {
        setStatus(`Найдено проблемных: ${s.missing + s.mismatched}. Нажмите «Восстановить».`);
        setBad(Array(s.missing + s.mismatched).fill("file"));
      }
      await runCheck().then(() => {});
    } catch (e) {
      setPhase("error");
      setStatus(`Ошибка проверки: ${e}`);
    }
  }

  async function runRepair() {
    setProgress(null);
    setPaused(false);
    setPhase("repairing");
    setStatus("Полная проверка и восстановление…");
    try {
      const s = await api.repair();
      setBad([]);
      setStatus(
        s.cancelled
          ? "Операция отменена"
          : s.missing + s.mismatched === 0
            ? `Все файлы целы (${s.ok})`
            : `Восстановлено: ${s.missing + s.mismatched}, целых ${s.ok}`,
      );
      await runCheck();
    } catch (e) {
      setPhase("error");
      setStatus(`Ошибка восстановления: ${e}`);
    }
  }

  async function runPlay() {
    setPhase("verifying");
    setStatus("Проверка перед запуском…");
    try {
      const r = await api.play();
      if (r.launched) {
        setPhase("playing");
        setStatus("Игра запущена. Доброй охоты!");
        setBad([]);
      } else {
        setBad(r.bad);
        setPhase("error");
        setStatus(`Целостность нарушена (${r.bad.length}). Нажмите «Восстановить».`);
      }
    } catch (e) {
      setPhase("error");
      setStatus(`Не удалось запустить: ${e}`);
    }
  }

  async function togglePause() {
    if (paused) {
      await api.resume();
      setPaused(false);
    } else {
      await api.pause();
      setPaused(true);
    }
  }

  async function doCancel() {
    await api.cancel();
  }

  async function pickInstallDir() {
    if (!config) return;
    const dir = await open({ directory: true, defaultPath: config.install_dir });
    if (typeof dir === "string") {
      const next = { ...config, install_dir: dir };
      setConfig(next);
      await api.saveConfig(next);
      await runCheck();
    }
  }

  const running = RUNNING.includes(phase);
  const busy = running || phase === "checking";
  const pct = progress && progress.total > 0 ? (progress.processed / progress.total) * 100 : 0;
  const showProgress = running && progress;

  return (
    <div className="flex h-full flex-col bg-[#0a0a0b] text-[#e9e4d8]">
      <TitleBar />

      <div className="relative flex-1 overflow-hidden">
        <Ambient />
        {/* тонкая решётка */}
        <div
          className="pointer-events-none absolute inset-0 opacity-[0.05]"
          style={{
            backgroundImage:
              "linear-gradient(to right, #c9a45c 1px, transparent 1px), linear-gradient(to bottom, #c9a45c 1px, transparent 1px)",
            backgroundSize: "56px 56px",
            maskImage: "radial-gradient(70% 60% at 50% 30%, #000 0%, transparent 75%)",
            WebkitMaskImage: "radial-gradient(70% 60% at 50% 30%, #000 0%, transparent 75%)",
          }}
        />

        <div className="reveal relative flex h-full flex-col items-center justify-center px-10 text-center">
          <StatusPill srv={srv} />

          <div className="mt-4 inline-flex items-center gap-2 rounded-full border border-[rgba(201,164,92,0.25)] bg-white/[0.03] px-4 py-1.5 text-[0.7rem] tracking-[0.2em] text-[rgba(201,164,92,0.9)] uppercase">
            Хроника Interlude · сборка {version} · Classic x1
          </div>
          <h1 className="mt-4 font-display text-6xl font-extrabold leading-none">
            <span className="shimmer-gold">INTERLUDE</span>
          </h1>
          <ul className="mt-7 flex items-center gap-7">
            {RATES.map((r) => (
              <li key={r.label} className="flex flex-col items-center">
                <span className="font-mono text-2xl font-semibold text-[#c9a45c]">{r.value}</span>
                <span className="text-[0.65rem] tracking-[0.18em] text-[rgba(233,228,216,0.45)] uppercase">
                  {r.label}
                </span>
              </li>
            ))}
          </ul>

          {bad.length > 0 && (
            <div className="mt-6 max-w-md rounded-xl border border-red-500/30 bg-red-500/[0.06] px-4 py-3 text-left text-xs text-red-200/90">
              <div className="mb-1 flex items-center gap-2 font-medium text-red-300">
                <AlertTriangle className="size-4" /> Нарушена целостность файлов
              </div>
              <ul className="max-h-20 overflow-auto font-mono text-[0.7rem] text-red-200/70">
                {bad.slice(0, 6).map((b, i) => (
                  <li key={i}>{b}</li>
                ))}
                {bad.length > 6 && <li>…и ещё {bad.length - 6}</li>}
              </ul>
            </div>
          )}
        </div>
      </div>

      {/* нижняя панель */}
      <div className="glass relative z-10 px-6 py-5">
        {showProgress && (
          <div className="mb-4">
            <div className="mb-1.5 flex items-center justify-between text-[0.7rem] tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
              <span>
                {progress!.phase === "verify" ? "Проверка целостности" : "Загрузка"}
                {paused && " · на паузе"}
              </span>
              <span>
                {progress!.files_done}/{progress!.files_total} файлов
              </span>
            </div>
            <div className="progress-track h-2.5">
              <div className="progress-fill" style={{ width: `${pct}%` }} />
            </div>
            <div className="mt-2 flex justify-between font-mono text-[0.7rem] text-[rgba(233,228,216,0.6)]">
              <span className="truncate pr-3">{progress!.current || "…"}</span>
              <span className="shrink-0">
                {fmtBytes(progress!.processed)} / {fmtBytes(progress!.total)} ·{" "}
                {paused ? "пауза" : `${fmtSpeed(progress!.speed_bps)} · ост. ${fmtEta(progress!.eta_secs)}`}
              </span>
            </div>
          </div>
        )}

        <div className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-3 text-sm">
            <StatusIcon phase={phase} paused={paused} />
            <span className="text-[rgba(233,228,216,0.8)]">{status}</span>
          </div>

          <div className="flex items-center gap-2">
            {running ? (
              <>
                <IconBtn title={paused ? "Возобновить" : "Пауза"} onClick={togglePause}>
                  {paused ? <Play className="size-4" /> : <Pause className="size-4" />}
                </IconBtn>
                <IconBtn title="Отменить" onClick={doCancel}>
                  <Square className="size-4" />
                </IconBtn>
              </>
            ) : (
              <>
                <IconBtn title="Проверить файлы" onClick={runVerify} disabled={busy}>
                  <ShieldCheck className="size-4" />
                </IconBtn>
                <IconBtn title="Настройки" onClick={() => setShowSettings(true)} disabled={busy}>
                  <SettingsIcon className="size-4" />
                </IconBtn>

                {phase === "outdated" ? (
                  <PrimaryBtn onClick={runUpdate} disabled={busy}>
                    <Download className="size-4" /> Обновить · {fmtBytes(bytesNeeded)}
                  </PrimaryBtn>
                ) : phase === "error" && bad.length > 0 ? (
                  <PrimaryBtn onClick={runRepair} disabled={busy}>
                    <RefreshCw className="size-4" /> Восстановить
                  </PrimaryBtn>
                ) : (
                  <PrimaryBtn onClick={runPlay} disabled={busy}>
                    <Play className="size-4" /> Играть
                  </PrimaryBtn>
                )}
              </>
            )}
          </div>
        </div>
      </div>

      {showSettings && config && (
        <Settings
          config={config}
          onPickDir={pickInstallDir}
          onChange={async (c) => {
            setConfig(c);
            await api.saveConfig(c);
          }}
          onClose={() => setShowSettings(false)}
        />
      )}
    </div>
  );
}

function StatusPill({ srv }: { srv: ServerInfo[] | null }) {
  if (!srv) {
    return <div className="text-xs text-[rgba(233,228,216,0.45)]">Проверка статуса серверов…</div>;
  }
  return (
    <div className="flex flex-wrap items-center justify-center gap-2">
      {srv.map((s) => {
        const color = s.online ? "#34d399" : "#c9a45c";
        return (
          <span
            key={s.id}
            className="inline-flex items-center gap-2 rounded-full border border-[rgba(201,164,92,0.2)] bg-white/[0.03] px-3.5 py-1.5 text-xs text-[rgba(233,228,216,0.75)]"
          >
            <span className="relative flex size-2">
              {s.online && (
                <span
                  className="absolute inline-flex size-2 rounded-full"
                  style={{ background: color, animation: "status-ping 1.6s cubic-bezier(0,0,0.2,1) infinite" }}
                />
              )}
              <span className="relative inline-flex size-2 rounded-full" style={{ background: color }} />
            </span>
            <span className="font-medium text-[rgba(233,228,216,0.9)]">{s.name}</span>
            <span className="text-[rgba(233,228,216,0.45)]">
              {s.online ? `${s.players}${s.max ? `/${s.max}` : ""}` : "оффлайн"}
            </span>
          </span>
        );
      })}
    </div>
  );
}

function StatusIcon({ phase, paused }: { phase: Phase; paused: boolean }) {
  if (paused) return <Pause className="size-4 text-[#c9a45c]" />;
  if (phase === "ready" || phase === "playing")
    return <CheckCircle2 className="size-4 text-[#c9a45c]" />;
  if (phase === "error") return <AlertTriangle className="size-4 text-red-400" />;
  return <Sigil className="size-4 pulse" />;
}

function PrimaryBtn({
  children,
  onClick,
  disabled,
}: {
  children: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="inline-flex items-center gap-2 rounded-xl bg-gradient-to-b from-[#e0c486] to-[#c9a45c] px-6 py-3 text-sm font-medium tracking-wide text-[#1a1407] shadow-[0_10px_30px_-10px_rgba(201,164,92,0.6)] transition-all hover:from-[#f0d59a] hover:to-[#d4af68] hover:-translate-y-0.5 disabled:cursor-not-allowed disabled:opacity-45 disabled:hover:translate-y-0"
    >
      {children}
    </button>
  );
}

function IconBtn({
  children,
  onClick,
  title,
  disabled,
}: {
  children: React.ReactNode;
  onClick: () => void;
  title: string;
  disabled?: boolean;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      disabled={disabled}
      className="grid size-11 place-items-center rounded-xl border border-[rgba(201,164,92,0.25)] bg-white/[0.02] text-[rgba(233,228,216,0.8)] transition-all hover:border-[rgba(201,164,92,0.5)] hover:text-[#c9a45c] disabled:cursor-not-allowed disabled:opacity-40"
    >
      {children}
    </button>
  );
}

function Settings({
  config,
  onPickDir,
  onChange,
  onClose,
}: {
  config: LauncherConfig;
  onPickDir: () => void;
  onChange: (c: LauncherConfig) => void;
  onClose: () => void;
}) {
  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="glass w-[440px] rounded-2xl p-6">
        <div className="mb-5 flex items-center justify-between">
          <h2 className="font-heading text-xl">Настройки</h2>
          <button
            onClick={onClose}
            className="grid size-8 place-items-center rounded-md text-[rgba(233,228,216,0.6)] hover:text-[#c9a45c]"
          >
            <X className="size-4" />
          </button>
        </div>

        <label className="mb-1 block text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
          Папка установки
        </label>
        <div className="mb-4 flex gap-2">
          <input
            readOnly
            value={config.install_dir}
            className="flex-1 truncate rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/30 px-3 py-2 font-mono text-xs text-[rgba(233,228,216,0.8)]"
          />
          <button
            onClick={onPickDir}
            className="grid size-9 place-items-center rounded-lg border border-[rgba(201,164,92,0.25)] hover:text-[#c9a45c]"
          >
            <FolderOpen className="size-4" />
          </button>
        </div>

        <label className="mb-1 block text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
          Параллельных загрузок
        </label>
        <input
          type="number"
          min={1}
          max={16}
          value={config.concurrency}
          onChange={(e) =>
            onChange({ ...config, concurrency: Math.max(1, Math.min(16, +e.target.value)) })
          }
          className="mb-2 w-24 rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/30 px-3 py-2 font-mono text-sm"
        />

        <p className="mt-4 text-[0.7rem] leading-relaxed text-[rgba(233,228,216,0.4)]">
          Сервер: {config.server_host}:{config.server_port}
        </p>
      </div>
    </div>
  );
}
