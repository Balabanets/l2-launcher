import { useEffect, useRef, useState } from "react";
import { Sparkles, Send, X, ArrowDown } from "lucide-react";
import { api } from "../lib/api";

type Msg = { role: "user" | "assistant"; content: string };

/** Ненавязчивые подсказки Mentis по кнопкам лаунчера — крутятся по кругу, рандомный порядок. */
const TIPS = [
  "Нажми «Играть», когда всё готово — я проверю файлы и запущу игру.",
  "Шестерёнка справа — путь установки, язык клиента и режим производительности.",
  "Иконка щита — проверю целостность клиента, если игра начнёт сбоить.",
  "Человечек с плюсом — создать игровой аккаунт, чтобы войти в мир.",
  "Жучок — отправить баг или идею прямо администрации сервера.",
  "Кликни по мне — и спроси что угодно про сервер: рейты, фарм, донат.",
];

// SSR-safe псевдо-рандом по индексу (без Math.random — детерминированно на цикл).
function pick(seed: number, len: number): number {
  const x = Math.sin((seed + 1) * 12.9898) * 43758.5453;
  return Math.floor((x - Math.floor(x)) * len);
}

/**
 * Mentis-ассистент лаунчера: периодические подсказки-«реплики» со стрелкой к кнопкам
 * (не перекрывают работу) + чат по клику (контрастная панель, не сливается с фоном).
 */
export function MentisAssistant({
  open,
  onClose,
  enabled,
}: {
  open: boolean;
  onClose: () => void;
  enabled: boolean;
}) {
  // ---- Подсказки ----
  const [tip, setTip] = useState<number | null>(null);
  const cycle = useRef(0);

  useEffect(() => {
    if (!enabled || open) {
      setTip(null);
      return;
    }
    if (typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
      return;
    }
    let alive = true;
    let hideT: ReturnType<typeof setTimeout>;
    const show = () => {
      if (!alive) return;
      setTip(pick(cycle.current++, TIPS.length));
      hideT = setTimeout(() => alive && setTip(null), 8500); // видно ~8.5с
    };
    const first = setTimeout(show, 3500);
    const loop = setInterval(show, 26000); // раз в ~26с
    return () => {
      alive = false;
      clearTimeout(first);
      clearTimeout(hideT);
      clearInterval(loop);
    };
  }, [enabled, open]);

  // ---- Чат ----
  const [messages, setMessages] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  }, [messages, busy]);

  useEffect(() => {
    if (open) {
      setTip(null);
      inputRef.current?.focus();
      const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
      window.addEventListener("keydown", onKey);
      return () => window.removeEventListener("keydown", onKey);
    }
  }, [open, onClose]);

  async function send() {
    const text = input.trim();
    if (!text || busy) return;
    const next = [...messages, { role: "user" as const, content: text }];
    setMessages(next);
    setInput("");
    setBusy(true);
    try {
      const reply = await api.assistantChat(next.slice(-20));
      setMessages((m) => [...m, { role: "assistant", content: reply }]);
    } catch {
      setMessages((m) => [
        ...m,
        { role: "assistant", content: "Не удалось связаться со мной. Проверь интернет или загляни в Discord." },
      ]);
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      {/* Подсказка-реплика: над нижней панелью, справа, со стрелкой вниз к кнопкам. */}
      {tip !== null && !open && (
        <div className="mentis-tip fixed right-6 bottom-[104px] z-50 w-[300px]">
          <div className="relative rounded-2xl border border-[rgba(201,164,92,0.35)] bg-[#12100b]/95 p-3.5 shadow-[0_18px_50px_-18px_rgba(0,0,0,0.9)] backdrop-blur-md">
            <button
              onClick={() => setTip(null)}
              aria-label="Скрыть подсказку"
              className="absolute right-2 top-2 grid size-5 place-items-center rounded text-[rgba(233,228,216,0.4)] transition hover:text-[#c9a45c]"
            >
              <X className="size-3.5" />
            </button>
            <div className="mb-1 flex items-center gap-1.5 text-[0.7rem] font-medium tracking-wide text-[#c9a45c] uppercase">
              <Sparkles className="size-3.5" /> Mentis
            </div>
            <p className="pr-3 text-xs leading-snug text-[rgba(233,228,216,0.85)]">{TIPS[tip]}</p>
            {/* стрелка вниз — к кнопкам нижней панели */}
            <ArrowDown className="absolute -bottom-4 right-6 size-5 text-[rgba(201,164,92,0.7)] mentis-bob" />
          </div>
        </div>
      )}

      {/* Чат-панель: контрастная (почти непрозрачная), не сливается с фоном. */}
      {open && (
        <>
          <div className="fixed inset-0 z-50" onClick={onClose} aria-hidden />
          <div className="mentis-chat fixed right-6 bottom-[104px] z-50 flex h-[440px] w-[360px] flex-col overflow-hidden rounded-2xl border border-[rgba(201,164,92,0.35)] bg-[#0e0f14] shadow-[0_30px_80px_-24px_rgba(0,0,0,0.95)]">
            <div className="flex items-center gap-2.5 border-b border-[rgba(201,164,92,0.15)] bg-[rgba(201,164,92,0.05)] px-4 py-3">
              <span className="grid size-8 shrink-0 place-items-center rounded-full border border-[rgba(201,164,92,0.35)] bg-[rgba(201,164,92,0.1)] text-[#c9a45c]">
                <Sparkles className="size-4" />
              </span>
              <div className="min-w-0">
                <div className="font-display text-sm tracking-wide text-gold-gradient">Mentis</div>
                <div className="truncate text-[0.7rem] text-[rgba(233,228,216,0.45)]">ИИ-ассистент LUNARGENT</div>
              </div>
              <button
                onClick={onClose}
                aria-label="Закрыть"
                className="ml-auto grid size-7 place-items-center rounded-lg text-[rgba(233,228,216,0.5)] transition hover:text-[#c9a45c]"
              >
                <X className="size-4" />
              </button>
            </div>

            <div ref={scrollRef} aria-live="polite" className="flex-1 space-y-3 overflow-y-auto px-4 py-3">
              <div className="rounded-xl bg-white/[0.04] px-3 py-2 text-sm text-[rgba(233,228,216,0.75)]">
                Привет! Я Mentis. Спроси про рейты, автофарм, донат, осады — или как пользоваться лаунчером.
              </div>
              {messages.map((m, i) => (
                <div
                  key={i}
                  className={
                    m.role === "user"
                      ? "ml-auto max-w-[85%] rounded-xl bg-[rgba(201,164,92,0.16)] px-3 py-2 text-sm text-[#e9e4d8]"
                      : "max-w-[85%] rounded-xl border-l-2 border-l-[rgba(96,181,255,0.3)] bg-white/[0.04] px-3 py-2 text-sm text-[rgba(233,228,216,0.85)]"
                  }
                >
                  {m.content}
                </div>
              ))}
              {busy && (
                <div className="flex gap-1 px-1">
                  {[0, 0.2, 0.4].map((d) => (
                    <span
                      key={d}
                      className="size-1.5 rounded-full bg-[#c9a45c]/70"
                      style={{ animation: `pulse-gold 1s ease-in-out ${d}s infinite` }}
                    />
                  ))}
                </div>
              )}
            </div>

            <form
              onSubmit={(e) => {
                e.preventDefault();
                send();
              }}
              className="flex items-center gap-2 border-t border-[rgba(201,164,92,0.15)] p-2.5"
            >
              <input
                ref={inputRef}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                placeholder="Спросить Mentis…"
                className="min-w-0 flex-1 rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/40 px-3 py-2 text-sm text-[#e9e4d8] outline-none transition focus:border-[rgba(201,164,92,0.5)]"
              />
              <button
                type="submit"
                disabled={busy || !input.trim()}
                aria-label="Отправить"
                className="grid size-9 shrink-0 place-items-center rounded-lg border border-[rgba(201,164,92,0.4)] bg-[rgba(201,164,92,0.1)] text-[#c9a45c] transition hover:bg-[rgba(201,164,92,0.2)] disabled:opacity-40"
              >
                <Send className="size-4" />
              </button>
            </form>
          </div>
        </>
      )}
    </>
  );
}
