import { useEffect, useState } from "react";
import { ShieldCheck, Settings as SettingsIcon, Bug, UserPlus, Sparkles } from "lucide-react";

const SEEN_KEY = "l2_hints_v1";

/** Одноразовая ненавязчивая подсказка по кнопкам нижней панели (показывается раз). */
export function Hints({ enabled }: { enabled: boolean }) {
  const [show, setShow] = useState(false);

  useEffect(() => {
    if (!enabled) return;
    let seen = false;
    try {
      seen = localStorage.getItem(SEEN_KEY) === "1";
    } catch {
      /* ignore */
    }
    if (!seen) {
      // Небольшая задержка, чтобы панель уже отрисовалась.
      const t = setTimeout(() => setShow(true), 600);
      return () => clearTimeout(t);
    }
  }, [enabled]);

  function dismiss() {
    try {
      localStorage.setItem(SEEN_KEY, "1");
    } catch {
      /* ignore */
    }
    setShow(false);
  }

  if (!show) return null;

  const items = [
    { icon: <UserPlus className="size-4" />, label: "Игровой аккаунт", text: "создать и управлять — нужен, чтобы зайти в игру" },
    { icon: <Bug className="size-4" />, label: "Сообщить о проблеме", text: "баг или идея → тикет администрации" },
    { icon: <ShieldCheck className="size-4" />, label: "Проверить файлы", text: "проверка целостности клиента" },
    { icon: <SettingsIcon className="size-4" />, label: "Настройки", text: "путь, язык, производительность, защита" },
  ];

  return (
    <>
      <div className="fixed inset-0 z-40" onClick={dismiss} />
      <div className="fixed bottom-24 right-6 z-50 w-[330px] rounded-2xl border border-[rgba(201,164,92,0.3)] bg-[#15130f] p-4 shadow-[0_20px_60px_-20px_rgba(0,0,0,0.85)]">
        <div className="mb-2.5 flex items-center gap-2 text-sm font-medium text-[#c9a45c]">
          <Sparkles className="size-4" /> Кнопки внизу
        </div>
        <div className="flex flex-col gap-2.5">
          {items.map((it) => (
            <div key={it.label} className="flex items-start gap-2.5">
              <span className="mt-0.5 grid size-7 shrink-0 place-items-center rounded-lg border border-[rgba(201,164,92,0.25)] bg-white/[0.03] text-[#c9a45c]">
                {it.icon}
              </span>
              <div className="text-xs leading-snug">
                <div className="text-[#e9e4d8]">{it.label}</div>
                <div className="text-[rgba(233,228,216,0.5)]">{it.text}</div>
              </div>
            </div>
          ))}
        </div>
        <button
          onClick={dismiss}
          className="mt-3.5 w-full rounded-lg bg-gradient-to-b from-[#e0c486] to-[#c9a45c] py-2 text-xs font-medium text-[#1a1407] transition hover:from-[#f0d59a] hover:to-[#d4af68]"
        >
          Понятно
        </button>
        {/* хвостик к кнопкам справа-внизу */}
        <div className="absolute -bottom-1.5 right-10 size-3 rotate-45 border-b border-r border-[rgba(201,164,92,0.3)] bg-[#15130f]" />
      </div>
    </>
  );
}
