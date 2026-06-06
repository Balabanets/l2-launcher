import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Play,
  ShieldCheck,
  RefreshCw,
  Settings as SettingsIcon,
  Download,
  FolderOpen,
  AlertTriangle,
  CheckCircle2,
  X,
} from "lucide-react";
import {
  api,
  onProgress,
  fmtBytes,
  fmtSpeed,
  fmtEta,
  type LauncherConfig,
  type Progress,
} from "./lib/api";
import { TitleBar } from "./components/TitleBar";
import { Sigil } from "./components/Sigil";

type Phase =
  | "checking"
  | "outdated"
  | "updating"
  | "ready"
  | "verifying"
  | "repairing"
  | "playing"
  | "error";

const RATES = [
  { label: "EXP", value: "x10" },
  { label: "SP", value: "x10" },
  { label: "Adena", value: "x5" },
  { label: "Drop", value: "x5" },
];

export default function App() {
  const [phase, setPhase] = useState<Phase>("checking");
  const [status, setStatus] = useState("Проверка обновлений…");
  const [version, setVersion] = useState("—");
  const [bytesNeeded, setBytesNeeded] = useState(0);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [bad, setBad] = useState<string[]>([]);
  const [config, setConfig] = useState<LauncherConfig | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const unlisten = useRef<(() => void) | null>(null);

  useEffect(() => {
    (async () => {
      unlisten.current = await onProgress(setProgress);
      try {
        setConfig(await api.getConfig());
      } catch {
        /* ignore */
      }
      await runCheck();
    })();
    return () => unlisten.current?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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

  async function runRepair() {
    setPhase("repairing");
    setStatus("Полная проверка целостности…");
    try {
      const s = await api.repair();
      setBad([]);
      setStatus(
        s.missing + s.mismatched === 0
          ? `Все файлы целы (${s.ok})`
          : `Восстановлено: ${s.missing + s.mismatched}, целых ${s.ok}`,
      );
      await runCheck();
    } catch (e) {
      setPhase("error");
      setStatus(`Ошибка проверки: ${e}`);
    }
  }

  async function runPlay() {
    setPhase("verifying");
    setStatus("Проверка целостности перед запуском…");
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

  const busy =
    phase === "updating" || phase === "repairing" || phase === "verifying" || phase === "checking";
  const pct = progress && progress.total > 0 ? (progress.downloaded / progress.total) * 100 : 0;

  return (
    <div className="flex h-full flex-col bg-[#0a0a0b] text-[#e9e4d8]">
      <TitleBar />

      {/* фон + герой */}
      <div className="relative flex-1 overflow-hidden">
        <div
          className="pointer-events-none absolute inset-0 opacity-60"
          style={{
            background:
              "radial-gradient(120% 80% at 50% -10%, rgba(201,164,92,0.10), transparent 60%), radial-gradient(80% 60% at 50% 120%, rgba(201,164,92,0.05), transparent 55%)",
          }}
        />
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

        <div className="relative flex h-full flex-col items-center justify-center px-10 text-center">
          <div className="mb-5 inline-flex items-center gap-2 rounded-full border border-[rgba(201,164,92,0.25)] bg-white/[0.03] px-4 py-1.5 text-[0.7rem] tracking-[0.2em] text-[rgba(201,164,92,0.9)] uppercase">
            <span className="size-1.5 rounded-full bg-[#c9a45c]" />
            Хроника Interlude · сборка {version}
          </div>
          <h1 className="font-display text-6xl font-extrabold leading-none">
            <span className="text-gold-gradient">INTERLUDE</span>
          </h1>
          <ul className="mt-7 flex items-center gap-7">
            {RATES.map((r) => (
              <li key={r.label} className="flex flex-col items-center">
                <span className="text-2xl font-semibold text-[#c9a45c]">{r.value}</span>
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
                {bad.slice(0, 6).map((b) => (
                  <li key={b}>{b}</li>
                ))}
                {bad.length > 6 && <li>…и ещё {bad.length - 6}</li>}
              </ul>
            </div>
          )}
        </div>
      </div>

      {/* нижняя панель управления */}
      <div className="glass relative z-10 px-6 py-5">
        {(phase === "updating" || phase === "repairing") && progress && (
          <div className="mb-4">
            <div className="progress-track h-2.5">
              <div className="progress-fill" style={{ width: `${pct}%` }} />
            </div>
            <div className="mt-2 flex justify-between font-mono text-[0.7rem] text-[rgba(233,228,216,0.6)]">
              <span className="truncate pr-3">{progress.current || "…"}</span>
              <span className="shrink-0">
                {fmtBytes(progress.downloaded)} / {fmtBytes(progress.total)} ·{" "}
                {fmtSpeed(progress.speed_bps)} · ост. {fmtEta(progress.eta_secs)}
              </span>
            </div>
          </div>
        )}

        <div className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-3 text-sm">
            <StatusIcon phase={phase} />
            <span className="text-[rgba(233,228,216,0.8)]">{status}</span>
          </div>

          <div className="flex items-center gap-2">
            <IconBtn title="Проверить файлы" onClick={runRepair} disabled={busy}>
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

function StatusIcon({ phase }: { phase: Phase }) {
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
