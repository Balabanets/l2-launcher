import { ExternalLink } from "lucide-react";

/** Окно ожидания подтверждения входа в браузере (device-code flow). */
export function LoginPrompt({ code, onCancel }: { code: string; onCancel: () => void }) {
  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 p-6 backdrop-blur-sm">
      <div className="glass w-[420px] rounded-2xl p-6 text-center">
        <div className="mx-auto mb-4 grid size-12 place-items-center rounded-full border border-[rgba(201,164,92,0.3)] bg-[rgba(201,164,92,0.08)]">
          <ExternalLink className="size-5 text-[#c9a45c]" />
        </div>
        <h2 className="font-heading text-xl">Подтвердите вход</h2>
        <p className="mt-2 text-sm text-[rgba(233,228,216,0.7)]">
          В браузере открылась страница входа. Войдите через Google или Discord и нажмите
          «Подтвердить вход в лаунчере».
        </p>
        <div className="mt-4 inline-flex items-center gap-2 rounded-lg border border-[rgba(201,164,92,0.25)] bg-black/30 px-4 py-2">
          <span className="text-[0.7rem] tracking-wide text-[rgba(233,228,216,0.5)] uppercase">Код</span>
          <span className="font-mono text-lg text-[#c9a45c]">{code}</span>
        </div>
        <p className="mt-4 text-xs text-[rgba(233,228,216,0.45)]">Ожидание подтверждения…</p>
        <button
          onClick={onCancel}
          className="mt-5 rounded-lg border border-[rgba(201,164,92,0.25)] px-4 py-2 text-sm text-[rgba(233,228,216,0.7)] transition hover:text-[#c9a45c]"
        >
          Отмена
        </button>
      </div>
    </div>
  );
}
