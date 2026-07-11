import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
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
  Server,
  Users,
  Clock,
  Gauge,
  Languages,
  ShieldAlert,
  ExternalLink,
  LogIn,
  Bug,
  UserPlus,
} from "lucide-react";
import {
  api,
  type ClientSettings,
  onProgress,
  fmtBytes,
  fmtSpeed,
  fmtEta,
  fmtUptime,
  type LauncherConfig,
  type Progress,
  type ServerInfo,
  type SelfUpdateInfo,
  type SacState,
  type Diagnostics,
  type LauncherUser,
} from "./lib/api";
import { TitleBar } from "./components/TitleBar";
import { Sigil } from "./components/Sigil";
import { Ambient } from "./components/Ambient";
import { BugReport } from "./components/BugReport";
import { GameAccountModal } from "./components/GameAccount";
import { LoginPrompt } from "./components/LoginPrompt";
import { ProfileMenu } from "./components/ProfileMenu";
import { MentisAssistant } from "./components/MentisAssistant";

type Phase =
  | "checking"
  | "outdated"
  | "updating"
  | "ready"
  | "repairing"
  | "verifying"
  | "playing"
  | "error";


// Операции с прогрессом, которые можно ставить на паузу/отменять.
const RUNNING: Phase[] = ["updating", "repairing", "verifying"];

export default function App() {
  const [phase, setPhase] = useState<Phase>("checking");
  const [status, setStatus] = useState("Проверка обновлений…");
  const [bytesNeeded, setBytesNeeded] = useState(0);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [paused, setPaused] = useState(false);
  const [bad, setBad] = useState<string[]>([]);
  const [config, setConfig] = useState<LauncherConfig | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  // Интро-анимация в главном фрейме при запуске (muted). Уважает reduced-motion.
  const [introDone, setIntroDone] = useState(
    () => typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches,
  );
  const [introFading, setIntroFading] = useState(false);
  const [srv, setSrv] = useState<ServerInfo[] | null>(null);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  const [selfUpd, setSelfUpd] = useState<SelfUpdateInfo | null>(null);
  const [updatingSelf, setUpdatingSelf] = useState(false);
  const [sac, setSac] = useState<SacState>("off");
  const [me, setMe] = useState<LauncherUser | null>(null);
  const [authState, setAuthState] = useState<"idle" | "waiting">("idle");
  const [authCode, setAuthCode] = useState<string | null>(null);
  const [authError, setAuthError] = useState<string | null>(null);
  const [showBug, setShowBug] = useState(false);
  const [showGameAcc, setShowGameAcc] = useState(false);
  const [showMentis, setShowMentis] = useState(false);
  const unlisten = useRef<(() => void) | null>(null);
  const loginAbort = useRef(false);

  // Живой тик аптайма раз в секунду.
  useEffect(() => {
    const id = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(id);
  }, []);

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
      // Тихо проверяем обновление лаунчера: если есть — показываем кнопку,
      // ничего не качаем и не ставим без действия игрока.
      try {
        const u = await api.checkSelfUpdate();
        if (u) setSelfUpd(u);
      } catch {
        /* оффлайн/dev — продолжаем со старой версией */
      }
      // Smart App Control (Windows 11): если включён — он заблокирует запуск игры.
      // Показываем гид по выключению, играть не даём, пока не выключат.
      try {
        setSac(await api.sacStatus());
      } catch {
        /* не Windows / нет данных — считаем выключенным */
      }
      // Кто вошёл (по сохранённому токену).
      try {
        setMe(await api.authMe());
      } catch {
        /* не вошёл / оффлайн */
      }
      await runCheck();
    })();
    return () => unlisten.current?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Живое обнаружение обновления лаунчера: опрос раз в 2 минуты, чтобы запущенный
  // лаунчер заметил новую версию без перезапуска. Файл launcher.json лежит на CDN
  // GitHub (не на нашем сервере) и не считается API-запросом — нагрузки на нашу
  // инфраструктуру нет даже при многих игроках. Не трогаем во время самого
  // самообновления. checkSelfUpdate возвращает null, если версия актуальна.
  useEffect(() => {
    let alive = true;
    const id = setInterval(() => {
      if (updatingSelf) return;
      api
        .checkSelfUpdate()
        .then((u) => alive && setSelfUpd(u))
        .catch(() => {});
    }, 120_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [updatingSelf]);

  // Живой статус сервера: опрос раз в 30с.
  useEffect(() => {
    let alive = true;
    const load = () =>
      api
        .serverStatus()
        .then((s) => alive && setSrv(s))
        .catch(() => {});
    load();
    const id = setInterval(load, 15_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  // Вход через сайт (OAuth, device-code): открыть браузер и опрашивать подтверждение.
  async function startLogin() {
    setAuthError(null);
    setAuthState("waiting");
    loginAbort.current = false;
    try {
      const b = await api.authBegin();
      setAuthCode(b.code);
      await openUrl(b.verify_url);
      const deadline = Date.now() + Math.min(b.expires_in, 600) * 1000;
      while (Date.now() < deadline && !loginAbort.current) {
        await new Promise((r) => setTimeout(r, 2500));
        if (loginAbort.current) break;
        let res;
        try {
          res = await api.authPoll(b.secret);
        } catch {
          continue;
        }
        if (res.status === "approved") {
          setMe(await api.authMe());
          setAuthState("idle");
          setAuthCode(null);
          return;
        }
        if (res.status === "expired") break;
      }
      if (!loginAbort.current) setAuthError("Время вышло. Войдите снова.");
    } catch (e) {
      setAuthError(String(e));
    } finally {
      setAuthState("idle");
      setAuthCode(null);
    }
  }

  function cancelLogin() {
    loginAbort.current = true;
    setAuthState("idle");
    setAuthCode(null);
  }

  async function logout() {
    try {
      await api.authLogout();
    } catch {
      /* ignore */
    }
    setMe(null);
  }

  // Применить обновление лаунчера по нажатию игрока: скачать → проверить
  // (SHA-256 + подпись) → заменить exe на месте → перезапуск. Прогресс идёт
  // через те же события update:progress. При успехе процесс перезапускается сам.
  async function runSelfUpdate() {
    setProgress(null);
    setUpdatingSelf(true);
    setPhase("updating");
    setStatus(`Обновление лаунчера до ${selfUpd?.version ?? ""}…`);
    try {
      await api.applySelfUpdate(); // не возвращается при успехе (перезапуск)
    } catch (e) {
      setUpdatingSelf(false);
      setPhase("error");
      setStatus(`Не удалось обновить лаунчер: ${e}`);
    }
  }

  // Открыть «Безопасность Windows» на странице с переключателем Smart App Control.
  async function openSac() {
    try {
      await api.openSacSettings();
    } catch {
      /* best-effort */
    }
  }

  // Перепроверить состояние SAC (после того как игрок выключил его в настройках).
  async function recheckSac() {
    try {
      setSac(await api.sacStatus());
    } catch {
      /* ignore */
    }
  }

  async function runCheck() {
    setPhase("checking");
    setStatus("Проверка обновлений…");
    try {
      const r = await api.checkUpdate();
      setBytesNeeded(r.bytes_to_download);
      if (r.needs_update) {
        setPhase("outdated");
        setStatus(`Доступно обновление · ${fmtBytes(r.bytes_to_download)}`);
      } else {
        setPhase("ready");
        setStatus("Готово — можно играть");
      }
    } catch (e) {
      setPhase("error");
      setStatus(`Ошибка проверки: ${e}`);
    }
  }

  async function runUpdate() {
    setProgress(null);
    setPaused(false);
    setBad([]); // обновление — это просто загрузка: не показываем список «битых» файлов
    setPhase("updating");
    setStatus("Загрузка и распаковка файлов…");
    try {
      await api.startUpdate();
      // Загрузка/распаковка завершены — короткий шаг применения настроек.
      setProgress(null);
      setStatus("Применение настроек клиента…");
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

  // Плавно завершить интро: fade-out 500мс, затем снять оверлей.
  const endIntro = () => {
    if (introFading || introDone) return;
    setIntroFading(true);
    setTimeout(() => setIntroDone(true), 750);
  };

  return (
    <div className="flex h-full flex-col bg-[#0a0a0b] text-[#e9e4d8]">
      <TitleBar />

      <div className="relative flex-1 overflow-hidden">
        {/* Фон как на сайте: сцена с замком (public/hero-base.jpg, cover) */}
        <div
          className="pointer-events-none absolute inset-0"
          style={{
            backgroundColor: "#0a0a0b",
            backgroundImage: "url('/hero-base.jpg')",
            backgroundRepeat: "no-repeat",
            backgroundPosition: "center center",
            backgroundSize: "cover",
          }}
        />
        {/* Скрим — темнее книзу, для читаемости контента (как на сайте) */}
        <div
          className="pointer-events-none absolute inset-0"
          style={{
            background:
              "linear-gradient(180deg, rgba(10,10,11,0.42) 0%, rgba(10,10,11,0.5) 30%, rgba(10,10,11,0.66) 68%, rgba(10,10,11,0.86) 100%)",
          }}
        />
        <div
          className="pointer-events-none absolute inset-0"
          style={{ background: "radial-gradient(130% 110% at 50% 34%, transparent 38%, rgba(10,10,11,0.6) 100%)" }}
        />
        <Ambient />

        <div
          className="absolute inset-0 z-10 transition-[opacity,transform,filter] duration-[900ms] ease-[cubic-bezier(0.22,1,0.36,1)]"
          style={{
            opacity: introDone || introFading ? 1 : 0,
            transform: introDone || introFading ? "scale(1)" : "scale(1.04)",
            filter: introDone || introFading ? "blur(0px)" : "blur(6px)",
          }}
        >
          {/* Mentis — справа внизу, лёгкое парение; клик открывает чат-ассистента */}
          <button
            type="button"
            onClick={() => setShowMentis((v) => !v)}
            aria-label="Открыть ассистента Mentis"
            className="group/mentis launch-float absolute bottom-0 right-1 z-0 h-[52%] max-h-[440px] w-auto cursor-pointer focus:outline-none"
          >
            <img
              src="/brand/mentis.webp"
              alt=""
              aria-hidden
              className="h-full w-auto object-contain transition-[filter,transform] duration-300 group-hover/mentis:scale-[1.03] group-hover/mentis:brightness-110"
            />
          </button>

          {/* Плашка «доступно обновление лаунчера» — сверху по центру */}
          {selfUpd && !updatingSelf && (
            <button
              onClick={runSelfUpdate}
              className="group absolute left-1/2 top-6 z-20 inline-flex -translate-x-1/2 items-center gap-2.5 rounded-full border border-[rgba(201,164,92,0.4)] bg-[rgba(201,164,92,0.08)] px-5 py-2 text-sm text-[#e9e4d8] backdrop-blur-md transition hover:bg-[rgba(201,164,92,0.16)]"
            >
              <Download className="size-4 text-[#c9a45c] transition group-hover:translate-y-0.5" />
              Доступно обновление лаунчера{" "}
              <span className="font-mono text-[#c9a45c]">{selfUpd.version}</span>
              <span className="ml-1 rounded-md bg-[rgba(201,164,92,0.18)] px-2 py-0.5 text-[0.7rem] tracking-wide text-[#c9a45c] uppercase">
                Обновить
              </span>
            </button>
          )}

          {/* Главный логотип LUNARGENT — фиксированные 30% высоты, строго по центру
              по горизонтали, вертикальный якорь ~2/3. */}
          <img
            src="/brand/logo-hero.webp"
            alt="LUNARGENT"
            className="pointer-events-none absolute left-1/2 top-[58%] z-10 h-[30%] w-auto -translate-x-1/2 -translate-y-1/2 object-contain"
          />

          {/* Карточки серверов — слева внизу */}
          <div className="absolute bottom-5 left-6 z-20 w-[min(20rem,42%)]">
            <ServerCards servers={srv} now={now} />
          </div>

          {bad.length > 0 && (
            <div className="absolute bottom-24 left-1/2 z-20 max-w-md -translate-x-1/2 rounded-xl border border-red-500/30 bg-red-500/[0.06] px-4 py-3 text-left text-xs text-red-200/90">
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

          {sac === "on" && (
            <div className="absolute left-1/2 top-1/2 z-20 max-w-lg -translate-x-1/2 -translate-y-1/2 rounded-xl border border-amber-500/40 bg-amber-500/[0.07] px-5 py-4 text-left text-amber-100/90 backdrop-blur-md">
              <div className="mb-2 flex items-center gap-2 font-medium text-amber-300">
                <ShieldAlert className="size-4" /> Smart App Control блокирует запуск игры
              </div>
              <p className="text-xs leading-relaxed text-amber-100/75">
                Это защита Windows 11 против неподписанных программ. Её нельзя обойти
                исключениями — нужно выключить вручную (для игрового ПК это нормально):
              </p>
              <ol className="mt-2 list-decimal space-y-0.5 pl-5 text-xs text-amber-100/80">
                <li>Откройте «Безопасность Windows» (кнопка ниже).</li>
                <li>«Управление приложениями и браузером» → «Параметры Smart App Control».</li>
                <li>Переключите в «Выкл», подтвердите.</li>
                <li>Вернитесь и нажмите «Проверить снова».</li>
              </ol>
              <p className="mt-2 text-[0.7rem] text-amber-200/55">
                Внимание: выключение необратимо (обратно — только переустановкой Windows).
              </p>
              <div className="mt-3 flex gap-2">
                <button
                  onClick={openSac}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-amber-400/40 bg-amber-400/10 px-3 py-1.5 text-xs text-amber-100 transition hover:bg-amber-400/20"
                >
                  <ExternalLink className="size-3.5" /> Открыть настройки Windows
                </button>
                <button
                  onClick={recheckSac}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-amber-400/20 px-3 py-1.5 text-xs text-amber-100/80 transition hover:bg-amber-400/10"
                >
                  <RefreshCw className="size-3.5" /> Проверить снова
                </button>
              </div>
            </div>
          )}
        </div>

        {/* Интро-анимация запуска — только в этом фрейме, поверх фона/контента.
            Нижнюю панель (статус/управление) не перекрывает — она отдельный сосед. */}
        {!introDone && (
          <div
            className="absolute inset-0 z-30 cursor-pointer overflow-hidden bg-black transition-opacity duration-700 ease-out"
            style={{ opacity: introFading ? 0 : 1 }}
            onClick={endIntro}
            role="button"
            tabIndex={0}
            aria-label="Пропустить заставку"
            onKeyDown={(e) => (e.key === "Enter" || e.key === "Escape" || e.key === " ") && endIntro()}
          >
            {/* Размытый фон-заполнитель: та же анимация, cover+blur — заполняет поля
                по бокам без искажения пропорций основного видео (без чёрных полос). */}
            <video
              src="/intro.mp4"
              autoPlay
              muted
              playsInline
              preload="auto"
              aria-hidden
              className="absolute inset-0 h-full w-full scale-110 object-cover blur-3xl brightness-[0.5]"
            />
            {/* Основное видео — целиком, без обрезки и без растяжения. */}
            <video
              src="/intro.mp4"
              autoPlay
              muted
              playsInline
              preload="auto"
              onEnded={endIntro}
              className="relative h-full w-full object-contain"
            />
          </div>
        )}
      </div>

      {/* нижняя панель */}
      <div className="glass relative z-10 px-6 py-5">
        {showProgress && (
          <div className="mb-4">
            <div className="mb-1.5 flex items-center justify-between text-[0.7rem] tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
              <span>
                {progress!.phase === "verify" ? "Проверка целостности" : "Загрузка и распаковка"}
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
                {paused
                  ? "пауза"
                  : progress!.speed_bps > 0 && progress!.eta_secs > 0
                    ? `${fmtSpeed(progress!.speed_bps)} · ост. ${fmtEta(progress!.eta_secs)}`
                    : "обработка…"}
              </span>
            </div>
          </div>
        )}

        <div className="grid grid-cols-[1fr_auto_1fr] items-center gap-4">
          {/* Слева: профиль (позже — баланс донат-коинов). */}
          <div className="flex min-h-[1.75rem] items-center justify-self-start">
            {me && <ProfileMenu me={me} onLogout={logout} />}
          </div>

          {/* По центру: статус. */}
          <div className="flex min-w-0 items-center justify-center gap-2.5 px-2 text-center">
            <StatusIcon phase={phase} paused={paused} />
            <span className="truncate text-sm tracking-wide text-[rgba(233,228,216,0.85)]">
              {status}
            </span>
            {authError && <span className="shrink-0 text-xs text-red-300">· {authError}</span>}
          </div>

          {/* Справа: действия. */}
          <div className="flex items-center justify-end gap-2 justify-self-end">
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
                {me && (
                  <>
                    <IconBtn title="Игровой аккаунт" onClick={() => setShowGameAcc(true)} disabled={busy}>
                      <UserPlus className="size-4" />
                    </IconBtn>
                    <IconBtn title="Сообщить о проблеме" onClick={() => setShowBug(true)} disabled={busy}>
                      <Bug className="size-4" />
                    </IconBtn>
                  </>
                )}
                <IconBtn title="Проверить файлы" onClick={runVerify} disabled={busy}>
                  <ShieldCheck className="size-4" />
                </IconBtn>
                <IconBtn title="Настройки" onClick={() => setShowSettings(true)} disabled={busy}>
                  <SettingsIcon className="size-4" />
                </IconBtn>

                {sac === "on" ? (
                  <PrimaryBtn onClick={openSac} disabled={busy}>
                    <ShieldAlert className="size-4" /> Выключить Smart App Control
                  </PrimaryBtn>
                ) : selfUpd && !updatingSelf ? (
                  <PrimaryBtn onClick={runSelfUpdate} disabled={busy}>
                    <Download className="size-4" /> Обновить лаунчер · {selfUpd.version}
                  </PrimaryBtn>
                ) : !me ? (
                  <PrimaryBtn onClick={startLogin} disabled={busy || authState === "waiting"}>
                    <LogIn className="size-4" />{" "}
                    {authState === "waiting" ? "Подтвердите в браузере…" : "Войти, чтобы играть"}
                  </PrimaryBtn>
                ) : phase === "outdated" ? (
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
          selfUpd={selfUpd}
          sac={sac}
          onPickDir={pickInstallDir}
          onChange={async (c) => {
            setConfig(c);
            await api.saveConfig(c);
          }}
          onClose={() => setShowSettings(false)}
          onSelfUpdate={() => {
            setShowSettings(false);
            runSelfUpdate();
          }}
          onOpenSac={openSac}
        />
      )}

      {authState === "waiting" && authCode && (
        <LoginPrompt code={authCode} onCancel={cancelLogin} />
      )}

      {showGameAcc && <GameAccountModal onClose={() => setShowGameAcc(false)} />}

      {showBug && <BugReport onClose={() => setShowBug(false)} />}

      <MentisAssistant
        open={showMentis}
        onClose={() => setShowMentis(false)}
        enabled={introDone && !running && !showSettings && !showGameAcc && !showBug}
      />
    </div>
  );
}

function ServerCards({ servers, now }: { servers: ServerInfo[] | null; now: number }) {
  if (!servers) {
    return <div className="text-xs text-[rgba(233,228,216,0.45)]">Проверка статуса серверов…</div>;
  }
  return (
    <div className="grid w-full max-w-lg grid-cols-2 gap-3">
      {servers.map((s) => {
        const color = s.online ? "#34d399" : "#c9a45c";
        return (
          <div key={s.id} className="glass rounded-xl px-4 py-3 text-left">
            <div className="flex items-center gap-2">
              <span className="relative flex size-2">
                {s.online && (
                  <span
                    className="absolute inline-flex size-2 rounded-full"
                    style={{ background: color, animation: "status-ping 1.6s cubic-bezier(0,0,0.2,1) infinite" }}
                  />
                )}
                <span className="relative inline-flex size-2 rounded-full" style={{ background: color }} />
              </span>
              <Server className="size-3.5 text-[#c9a45c]" />
              <span className="truncate text-sm font-medium text-[rgba(233,228,216,0.9)]">{s.name}</span>
            </div>
            <div className="mt-2 flex items-center justify-between text-xs text-[rgba(233,228,216,0.55)]">
              <span className="flex items-center gap-1.5">
                <Users className="size-3" /> Игроки
              </span>
              <span className="font-mono text-[rgba(233,228,216,0.85)]">
                {s.online ? `${s.players}${s.max ? `/${s.max}` : ""}` : "—"}
              </span>
            </div>
            <div className="mt-1 flex items-center justify-between text-xs text-[rgba(233,228,216,0.55)]">
              <span className="flex items-center gap-1.5">
                <Clock className="size-3" /> Аптайм
              </span>
              <span className="font-mono text-[rgba(233,228,216,0.85)]">
                {s.online && s.started_at > 0 ? fmtUptime(s.started_at, now) : "—"}
              </span>
            </div>
          </div>
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
  selfUpd,
  sac,
  onPickDir,
  onChange,
  onClose,
  onSelfUpdate,
  onOpenSac,
}: {
  config: LauncherConfig;
  selfUpd: SelfUpdateInfo | null;
  sac: SacState;
  onPickDir: () => void;
  onChange: (c: LauncherConfig) => void;
  onClose: () => void;
  onSelfUpdate: () => void;
  onOpenSac: () => void;
}) {
  const [cs, setCs] = useState<ClientSettings | null>(null);
  const [csBusy, setCsBusy] = useState(false);
  const [csError, setCsError] = useState<string | null>(null);
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  const [diagLoading, setDiagLoading] = useState(true);

  function loadDiag() {
    setDiagLoading(true);
    api
      .diagnostics()
      .then(setDiag)
      .catch(() => {})
      .finally(() => setDiagLoading(false));
  }

  useEffect(() => {
    api.getClientSettings().then(setCs).catch(() => {});
    loadDiag();
  }, []);

  // Свежее состояние SAC из diagnostics (обновляется кнопкой), откат на проп.
  const sacState: SacState = diag?.sac ?? sac;

  async function applyPerf(enabled: boolean) {
    setCsBusy(true);
    setCsError(null);
    try {
      await api.setPerformanceMode(enabled);
      setCs(await api.getClientSettings());
    } catch (e) {
      setCsError(String(e));
    } finally {
      setCsBusy(false);
    }
  }
  async function applyLang(lang: string) {
    setCsBusy(true);
    setCsError(null);
    try {
      await api.setClientLanguage(lang);
      setCs(await api.getClientSettings());
    } catch (e) {
      setCsError(String(e));
    } finally {
      setCsBusy(false);
    }
  }

  const sacDetail =
    sacState === "off"
      ? "Выключен — не мешает запуску"
      : sacState === "on"
        ? "Включён — блокирует игру"
        : sacState === "evaluation"
          ? "Режим оценки"
          : "Состояние неизвестно";
  const sacLevel: HealthLevel =
    sacState === "off"
      ? "ok"
      : sacState === "on"
        ? "err"
        : sacState === "evaluation"
          ? "warn"
          : "idle";

  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 p-6 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="glass max-h-full w-[760px] overflow-y-auto rounded-2xl p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-5 flex items-center justify-between">
          <h2 className="font-heading text-xl">Настройки</h2>
          <button
            onClick={onClose}
            className="grid size-8 place-items-center rounded-md text-[rgba(233,228,216,0.6)] hover:text-[#c9a45c]"
          >
            <X className="size-4" />
          </button>
        </div>

        <div className="grid grid-cols-2 gap-7">
          {/* Левая колонка: установка + клиент */}
          <div>
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
                className="grid size-9 shrink-0 place-items-center rounded-lg border border-[rgba(201,164,92,0.25)] hover:text-[#c9a45c]"
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
              className="w-24 rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/30 px-3 py-2 font-mono text-sm"
            />

            <div className="mt-5 mb-3 text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
              Настройки клиента
            </div>

            {/* Режим производительности */}
            <label className="flex items-start gap-3">
              <input
                type="checkbox"
                checked={cs?.performance ?? false}
                disabled={!cs || csBusy}
                onChange={(e) => applyPerf(e.target.checked)}
                className="mt-1 size-4 accent-[#c9a45c]"
              />
              <span>
                <span className="flex items-center gap-1.5 text-sm text-[#e9e4d8]">
                  <Gauge className="size-4 text-[#c9a45c]" /> Режим производительности
                </span>
                <span className="block text-xs text-[rgba(233,228,216,0.45)]">
                  dgVoodoo + увеличенный кэш. Для современных ПК; на слабых выключить.
                </span>
              </span>
            </label>

            {/* Язык клиента */}
            <div className="mt-4 flex items-center gap-3">
              <span className="flex items-center gap-1.5 text-sm text-[#e9e4d8]">
                <Languages className="size-4 text-[#c9a45c]" /> Язык клиента
              </span>
              <div className="inline-flex rounded-lg border border-[rgba(201,164,92,0.2)] p-0.5">
                {(["ru", "en"] as const).map((l) => (
                  <button
                    key={l}
                    disabled={!cs || csBusy}
                    onClick={() => applyLang(l)}
                    className={`rounded-md px-3 py-1 text-xs uppercase transition disabled:opacity-50 ${
                      cs?.language === l
                        ? "bg-[rgba(201,164,92,0.18)] text-[#c9a45c]"
                        : "text-[rgba(233,228,216,0.6)]"
                    }`}
                  >
                    {l}
                  </button>
                ))}
              </div>
            </div>

            {csError && <p className="mt-3 text-xs text-red-300">{csError}</p>}
            <p className="mt-3 text-[0.7rem] text-[rgba(233,228,216,0.4)]">
              Изменения применяются только при закрытой игре.
            </p>
          </div>

          {/* Правая колонка: защита и версии */}
          <div className="border-l border-[rgba(201,164,92,0.12)] pl-7">
            <div className="mb-3 flex items-center justify-between">
              <span className="text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
                Защита и версии
              </span>
              <button
                onClick={loadDiag}
                title="Обновить"
                className="grid size-7 place-items-center rounded-md text-[rgba(233,228,216,0.5)] hover:text-[#c9a45c]"
              >
                <RefreshCw className={`size-3.5 ${diagLoading ? "animate-spin" : ""}`} />
              </button>
            </div>

            <div className="space-y-1.5">
              <StatusRow
                icon={<Download className="size-4" />}
                label="Лаунчер"
                detail={
                  selfUpd
                    ? `Доступно обновление ${selfUpd.version}`
                    : `Актуален · ${diag?.launcher_version ?? "—"}`
                }
                level={selfUpd ? "warn" : "ok"}
                action={selfUpd ? { label: "Обновить", onClick: onSelfUpdate } : undefined}
              />
              <StatusRow
                icon={<ShieldCheck className="size-4" />}
                label="Подпись клиента"
                detail={
                  diag?.manifest_signature_ok === true
                    ? "Ed25519 — валидна"
                    : "Не удалось проверить (оффлайн?)"
                }
                level={diag?.manifest_signature_ok === true ? "ok" : "warn"}
              />
              <StatusRow
                icon={<ShieldAlert className="size-4" />}
                label="Smart App Control"
                detail={sacDetail}
                level={sacLevel}
                action={sacState === "on" ? { label: "Выключить", onClick: onOpenSac } : undefined}
              />
              <StatusRow
                icon={<ShieldCheck className="size-4" />}
                label="Антивирус (Defender)"
                detail={
                  diag?.defender_excluded
                    ? "Папка игры в исключениях"
                    : "Исключение добавится при обновлении"
                }
                level={diag?.defender_excluded ? "ok" : "warn"}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

type HealthLevel = "ok" | "warn" | "err" | "idle";

function levelColor(l: HealthLevel): string {
  return l === "ok"
    ? "#34d399"
    : l === "warn"
      ? "#c9a45c"
      : l === "err"
        ? "#f87171"
        : "rgba(233,228,216,0.4)";
}

// Одна строка статуса (защита/версии) в «Настройках»: иконка + подпись + деталь,
// справа — действие или цветная точка состояния.
function StatusRow({
  icon,
  label,
  detail,
  level,
  action,
}: {
  icon: React.ReactNode;
  label: string;
  detail: string;
  level: HealthLevel;
  action?: { label: string; onClick: () => void };
}) {
  const c = levelColor(level);
  return (
    <div className="flex items-center gap-3 rounded-lg border border-[rgba(201,164,92,0.12)] bg-white/[0.02] px-3 py-2.5">
      <span
        className="grid size-7 shrink-0 place-items-center rounded-md"
        style={{ color: c, background: `${c}1a` }}
      >
        {icon}
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-sm text-[#e9e4d8]">{label}</div>
        <div className="truncate text-xs text-[rgba(233,228,216,0.5)]">{detail}</div>
      </div>
      {action ? (
        <button
          onClick={action.onClick}
          className="shrink-0 rounded-lg border border-[rgba(201,164,92,0.3)] px-3 py-1.5 text-xs text-[#c9a45c] transition hover:bg-[rgba(201,164,92,0.12)]"
        >
          {action.label}
        </button>
      ) : (
        <span className="size-2 shrink-0 rounded-full" style={{ background: c }} />
      )}
    </div>
  );
}
