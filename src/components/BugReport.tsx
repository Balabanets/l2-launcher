import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { X, Bug, Paperclip, Trash2, Check } from "lucide-react";
import { api } from "../lib/api";

/** 2-уровневая таксономия (совпадает с сайтом, ru.json: ticketMain/ticketSub). */
const TAXONOMY: { main: string; label: string; subs: [string, string][] }[] = [
  {
    main: "CLIENT",
    label: "Клиент / Лаунчер",
    subs: [
      ["client_bug", "Ошибка или вылет"],
      ["client_visual", "Графика и визуал"],
      ["client_idea", "Предложение"],
    ],
  },
  {
    main: "SERVER",
    label: "Игровой сервер",
    subs: [
      ["server_bug", "Баг механики или квеста"],
      ["server_balance", "Баланс и геймплей"],
      ["server_economy", "Экономика"],
      ["server_idea", "Предложение"],
    ],
  },
  {
    main: "ACCOUNT",
    label: "Аккаунт и доступ",
    subs: [
      ["acc_login", "Не могу войти"],
      ["acc_recovery", "Восстановление доступа"],
      ["acc_other", "Другое"],
    ],
  },
  {
    main: "ADMIN",
    label: "Администрация",
    subs: [
      ["adm_complaint", "Жалоба"],
      ["adm_question", "Вопрос или прочее"],
    ],
  },
];

const MAX_FILES = 6;
const baseName = (p: string) => p.split(/[/\\]/).pop() ?? p;

/** Модалка баг-репорта: раздел → подкатегория → заголовок → текст → файлы → отправка. */
export function BugReport({ onClose }: { onClose: () => void }) {
  const [category, setCategory] = useState("CLIENT");
  const [subcategory, setSubcategory] = useState("client_bug");
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [files, setFiles] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ticketId, setTicketId] = useState<number | null>(null);

  const subs = TAXONOMY.find((t) => t.main === category)?.subs ?? [];
  const canSubmit = title.trim().length >= 3 && description.trim().length >= 5 && !busy;

  function pickCategory(main: string) {
    setCategory(main);
    const first = TAXONOMY.find((t) => t.main === main)?.subs[0]?.[0];
    if (first) setSubcategory(first);
  }

  async function addFiles() {
    const sel = await open({
      multiple: true,
      filters: [
        { name: "Скриншоты и логи", extensions: ["png", "jpg", "jpeg", "webp", "gif", "txt", "log", "json"] },
      ],
    });
    if (!sel) return;
    const list = Array.isArray(sel) ? sel : [sel];
    setFiles((prev) => [...prev, ...list].slice(0, MAX_FILES));
  }

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      const res = await api.submitBugReport(category, subcategory, title.trim(), description.trim(), files);
      setTicketId(res.ticket_id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 p-6 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="glass max-h-full w-[560px] overflow-y-auto rounded-2xl p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="flex items-center gap-2 font-heading text-xl">
            <Bug className="size-5 text-[#c9a45c]" /> Сообщить о проблеме
          </h2>
          <button
            onClick={onClose}
            className="grid size-8 place-items-center rounded-md text-[rgba(233,228,216,0.6)] hover:text-[#c9a45c]"
          >
            <X className="size-4" />
          </button>
        </div>

        {ticketId !== null ? (
          <div className="py-6 text-center">
            <div className="mx-auto mb-3 grid size-12 place-items-center rounded-full border border-emerald-500/30 bg-emerald-500/10">
              <Check className="size-6 text-emerald-400" />
            </div>
            <p className="text-sm text-[#e9e4d8]">
              Спасибо! Создан тикет <span className="font-mono text-[#c9a45c]">#{ticketId}</span>.
            </p>
            <p className="mt-1 text-xs text-[rgba(233,228,216,0.5)]">
              Следить за ответом можно в личном кабинете на сайте.
            </p>
            <button
              onClick={onClose}
              className="mt-5 rounded-xl bg-gradient-to-b from-[#e0c486] to-[#c9a45c] px-6 py-2.5 text-sm font-medium text-[#1a1407]"
            >
              Готово
            </button>
          </div>
        ) : (
          <>
            <div className="grid grid-cols-2 gap-3">
              <label className="block">
                <span className="mb-1 block text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
                  Раздел
                </span>
                <select
                  value={category}
                  onChange={(e) => pickCategory(e.target.value)}
                  className="w-full rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/40 px-3 py-2 text-sm text-[#e9e4d8] outline-none focus:border-[rgba(201,164,92,0.5)]"
                >
                  {TAXONOMY.map((t) => (
                    <option key={t.main} value={t.main}>
                      {t.label}
                    </option>
                  ))}
                </select>
              </label>
              <label className="block">
                <span className="mb-1 block text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
                  Тип
                </span>
                <select
                  value={subcategory}
                  onChange={(e) => setSubcategory(e.target.value)}
                  className="w-full rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/40 px-3 py-2 text-sm text-[#e9e4d8] outline-none focus:border-[rgba(201,164,92,0.5)]"
                >
                  {subs.map(([id, label]) => (
                    <option key={id} value={id}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            <label className="mt-3 block">
              <span className="mb-1 block text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
                Заголовок
              </span>
              <input
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                maxLength={160}
                placeholder="Коротко о проблеме"
                className="w-full rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/30 px-3 py-2 text-sm text-[#e9e4d8] outline-none focus:border-[rgba(201,164,92,0.5)]"
              />
            </label>

            <label className="mt-3 block">
              <span className="mb-1 block text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
                Описание
              </span>
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                maxLength={8000}
                rows={5}
                placeholder="Что произошло, что делали до этого, что ожидали"
                className="w-full resize-none rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/30 px-3 py-2 text-sm text-[#e9e4d8] outline-none focus:border-[rgba(201,164,92,0.5)]"
              />
            </label>

            <div className="mt-3">
              <div className="mb-1.5 flex items-center justify-between">
                <span className="text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
                  Файлы (скриншоты, логи) · до {MAX_FILES}
                </span>
                <button
                  onClick={addFiles}
                  disabled={files.length >= MAX_FILES}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-[rgba(201,164,92,0.3)] px-2.5 py-1 text-xs text-[#c9a45c] transition hover:bg-[rgba(201,164,92,0.12)] disabled:opacity-40"
                >
                  <Paperclip className="size-3.5" /> Добавить
                </button>
              </div>
              {files.length > 0 && (
                <div className="flex flex-col gap-1">
                  {files.map((f, i) => (
                    <div
                      key={`${f}-${i}`}
                      className="flex items-center gap-2 rounded-lg border border-[rgba(201,164,92,0.12)] bg-white/[0.02] px-3 py-1.5 text-xs"
                    >
                      <Paperclip className="size-3.5 shrink-0 text-[rgba(233,228,216,0.5)]" />
                      <span className="min-w-0 flex-1 truncate font-mono text-[rgba(233,228,216,0.75)]">
                        {baseName(f)}
                      </span>
                      <button
                        onClick={() => setFiles((prev) => prev.filter((_, j) => j !== i))}
                        className="text-[rgba(233,228,216,0.45)] transition hover:text-red-300"
                      >
                        <Trash2 className="size-3.5" />
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {error && <p className="mt-3 text-xs text-red-300">{error}</p>}

            <div className="mt-5 flex justify-end gap-2">
              <button
                onClick={onClose}
                className="rounded-lg border border-[rgba(201,164,92,0.2)] px-4 py-2 text-sm text-[rgba(233,228,216,0.7)] transition hover:text-[#c9a45c]"
              >
                Отмена
              </button>
              <button
                onClick={submit}
                disabled={!canSubmit}
                className="inline-flex items-center gap-2 rounded-xl bg-gradient-to-b from-[#e0c486] to-[#c9a45c] px-6 py-2 text-sm font-medium text-[#1a1407] transition hover:from-[#f0d59a] hover:to-[#d4af68] disabled:cursor-not-allowed disabled:opacity-45"
              >
                <Bug className="size-4" /> {busy ? "Отправка…" : "Отправить"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
