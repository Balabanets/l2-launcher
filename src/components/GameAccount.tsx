import { useEffect, useState } from "react";
import { X, UserPlus, Check } from "lucide-react";
import { api, type GameAccount } from "../lib/api";

/** Модалка игровых аккаунтов: список существующих + создание (login/password). */
export function GameAccountModal({ onClose }: { onClose: () => void }) {
  const [accounts, setAccounts] = useState<GameAccount[] | null>(null);
  const [login, setLogin] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<string | null>(null);

  function load() {
    api
      .listGameAccounts()
      .then(setAccounts)
      .catch(() => setAccounts([]));
  }
  useEffect(load, []);

  async function create() {
    setBusy(true);
    setError(null);
    setCreated(null);
    try {
      const l = await api.createGameAccount(login.trim(), password);
      setCreated(l);
      setLogin("");
      setPassword("");
      load();
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
      <div className="glass w-[460px] rounded-2xl p-6" onClick={(e) => e.stopPropagation()}>
        <div className="mb-4 flex items-center justify-between">
          <h2 className="font-heading text-xl">Игровой аккаунт</h2>
          <button
            onClick={onClose}
            className="grid size-8 place-items-center rounded-md text-[rgba(233,228,216,0.6)] hover:text-[#c9a45c]"
          >
            <X className="size-4" />
          </button>
        </div>

        <p className="mb-4 text-xs text-[rgba(233,228,216,0.5)]">
          Этим логином и паролем вы заходите в игру. Можно создать несколько.
        </p>

        {accounts && accounts.length > 0 && (
          <div className="mb-4">
            <div className="mb-2 text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
              Ваши аккаунты
            </div>
            <div className="flex flex-col gap-1.5">
              {accounts.map((a) => (
                <div
                  key={a.login}
                  className="flex items-center gap-2 rounded-lg border border-[rgba(201,164,92,0.12)] bg-white/[0.02] px-3 py-2 text-sm"
                >
                  <Check className="size-4 text-emerald-400" />
                  <span className="font-mono text-[#e9e4d8]">{a.login}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="border-t border-[rgba(201,164,92,0.15)] pt-4">
          <div className="mb-3 text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
            Создать аккаунт
          </div>
          <input
            value={login}
            onChange={(e) => setLogin(e.target.value)}
            placeholder="Логин"
            autoCapitalize="off"
            autoCorrect="off"
            className="mb-2 w-full rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/30 px-3 py-2 font-mono text-sm text-[#e9e4d8] outline-none focus:border-[rgba(201,164,92,0.5)]"
          />
          <input
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Пароль"
            type="password"
            className="w-full rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/30 px-3 py-2 font-mono text-sm text-[#e9e4d8] outline-none focus:border-[rgba(201,164,92,0.5)]"
          />
          {error && <p className="mt-2 text-xs text-red-300">{error}</p>}
          {created && (
            <p className="mt-2 text-xs text-emerald-300">
              Аккаунт <span className="font-mono">{created}</span> создан — можно играть.
            </p>
          )}
          <button
            onClick={create}
            disabled={busy || login.trim().length === 0 || password.length === 0}
            className="mt-3 inline-flex items-center gap-2 rounded-xl bg-gradient-to-b from-[#e0c486] to-[#c9a45c] px-5 py-2.5 text-sm font-medium text-[#1a1407] transition hover:from-[#f0d59a] hover:to-[#d4af68] disabled:cursor-not-allowed disabled:opacity-45"
          >
            <UserPlus className="size-4" /> {busy ? "Создаю…" : "Создать"}
          </button>
        </div>
      </div>
    </div>
  );
}
