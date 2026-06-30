import { useEffect, useState } from "react";
import { X, UserPlus, KeyRound, Check, Plus, Loader2, Link2 } from "lucide-react";
import { api, type GameAccount } from "../lib/api";

/** Игровые аккаунты: список текущих + смена пароля + добавление. */
export function GameAccountModal({ onClose }: { onClose: () => void }) {
  const [accounts, setAccounts] = useState<GameAccount[] | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [editPwd, setEditPwd] = useState("");
  const [rowBusy, setRowBusy] = useState(false);
  const [rowMsg, setRowMsg] = useState<{ login: string; ok: boolean; text: string } | null>(null);

  const [mode, setMode] = useState<"none" | "create" | "claim">("none");
  const [login, setLogin] = useState("");
  const [password, setPassword] = useState("");
  const [createBusy, setCreateBusy] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);

  function load() {
    api
      .listGameAccounts()
      .then(setAccounts)
      .catch(() => setAccounts([]));
  }
  useEffect(load, []);

  async function savePassword(acc: string) {
    setRowBusy(true);
    setRowMsg(null);
    try {
      await api.changeGameAccountPassword(acc, editPwd);
      setRowMsg({ login: acc, ok: true, text: "Пароль изменён" });
      setEditing(null);
      setEditPwd("");
    } catch (e) {
      setRowMsg({ login: acc, ok: false, text: String(e) });
    } finally {
      setRowBusy(false);
    }
  }

  function reset() {
    setLogin("");
    setPassword("");
    setMode("none");
    setCreateError(null);
  }

  async function submitForm() {
    setCreateBusy(true);
    setCreateError(null);
    try {
      if (mode === "claim") {
        await api.claimGameAccount(login.trim(), password);
      } else {
        await api.createGameAccount(login.trim(), password);
      }
      reset();
      load();
    } catch (e) {
      setCreateError(String(e));
    } finally {
      setCreateBusy(false);
    }
  }

  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 p-6 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="glass max-h-full w-[480px] overflow-y-auto rounded-2xl p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="font-heading text-xl">Игровые аккаунты</h2>
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

        {/* Список */}
        {accounts === null ? (
          <div className="flex items-center gap-2 py-6 text-sm text-[rgba(233,228,216,0.5)]">
            <Loader2 className="size-4 animate-spin" /> Загрузка…
          </div>
        ) : accounts.length === 0 ? (
          <div className="rounded-xl border border-dashed border-[rgba(201,164,92,0.25)] px-4 py-6 text-center text-sm text-[rgba(233,228,216,0.55)]">
            Пока нет игровых аккаунтов. Создайте первый — он нужен, чтобы зайти в игру.
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            {accounts.map((a) => (
              <div
                key={a.login}
                className="rounded-xl border border-[rgba(201,164,92,0.14)] bg-white/[0.02] px-3 py-2.5"
              >
                <div className="flex items-center gap-2">
                  <span className="grid size-7 place-items-center rounded-lg bg-[rgba(201,164,92,0.14)] text-[#c9a45c]">
                    <UserPlus className="size-4" />
                  </span>
                  <span className="flex-1 truncate font-mono text-sm text-[#e9e4d8]">{a.login}</span>
                  <button
                    onClick={() => {
                      setEditing(editing === a.login ? null : a.login);
                      setEditPwd("");
                      setRowMsg(null);
                    }}
                    className="inline-flex items-center gap-1.5 rounded-lg border border-[rgba(201,164,92,0.25)] px-2.5 py-1 text-xs text-[rgba(233,228,216,0.8)] transition hover:border-[rgba(201,164,92,0.5)] hover:text-[#c9a45c]"
                  >
                    <KeyRound className="size-3.5" /> Пароль
                  </button>
                </div>

                {editing === a.login && (
                  <div className="mt-2.5 flex gap-2">
                    <input
                      value={editPwd}
                      onChange={(e) => setEditPwd(e.target.value)}
                      type="password"
                      placeholder="Новый пароль"
                      className="flex-1 rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/30 px-3 py-1.5 font-mono text-sm text-[#e9e4d8] outline-none focus:border-[rgba(201,164,92,0.5)]"
                    />
                    <button
                      onClick={() => savePassword(a.login)}
                      disabled={rowBusy || editPwd.length === 0}
                      className="inline-flex items-center gap-1.5 rounded-lg bg-[rgba(201,164,92,0.18)] px-3 py-1.5 text-xs text-[#c9a45c] transition hover:bg-[rgba(201,164,92,0.28)] disabled:opacity-40"
                    >
                      {rowBusy ? <Loader2 className="size-3.5 animate-spin" /> : <Check className="size-3.5" />}
                      Сохранить
                    </button>
                  </div>
                )}
                {rowMsg?.login === a.login && (
                  <p className={`mt-2 text-xs ${rowMsg.ok ? "text-emerald-300" : "text-red-300"}`}>
                    {rowMsg.text}
                  </p>
                )}
              </div>
            ))}
          </div>
        )}

        {/* Добавление / привязка */}
        <div className="mt-4 border-t border-[rgba(201,164,92,0.15)] pt-4">
          {mode === "none" ? (
            <div className="flex flex-wrap gap-2">
              <button
                onClick={() => setMode("create")}
                className="inline-flex items-center gap-2 rounded-lg border border-[rgba(201,164,92,0.3)] px-4 py-2 text-sm text-[#c9a45c] transition hover:bg-[rgba(201,164,92,0.12)]"
              >
                <Plus className="size-4" /> Создать новый
              </button>
              <button
                onClick={() => setMode("claim")}
                className="inline-flex items-center gap-2 rounded-lg border border-[rgba(201,164,92,0.2)] px-4 py-2 text-sm text-[rgba(233,228,216,0.8)] transition hover:border-[rgba(201,164,92,0.5)] hover:text-[#c9a45c]"
              >
                <Link2 className="size-4" /> Привязать существующий
              </button>
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <div className="text-xs tracking-wide text-[rgba(233,228,216,0.55)] uppercase">
                {mode === "claim" ? "Привязать существующий аккаунт" : "Новый аккаунт"}
              </div>
              {mode === "claim" && (
                <p className="text-xs text-[rgba(233,228,216,0.5)]">
                  Введите логин и пароль уже существующего игрового аккаунта — он привяжется
                  к вашему профилю.
                </p>
              )}
              <input
                value={login}
                onChange={(e) => setLogin(e.target.value)}
                placeholder="Логин"
                autoCapitalize="off"
                autoCorrect="off"
                className="w-full rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/30 px-3 py-2 font-mono text-sm text-[#e9e4d8] outline-none focus:border-[rgba(201,164,92,0.5)]"
              />
              <input
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                type="password"
                placeholder="Пароль"
                className="w-full rounded-lg border border-[rgba(201,164,92,0.2)] bg-black/30 px-3 py-2 font-mono text-sm text-[#e9e4d8] outline-none focus:border-[rgba(201,164,92,0.5)]"
              />
              {createError && <p className="text-xs text-red-300">{createError}</p>}
              <div className="flex gap-2">
                <button
                  onClick={submitForm}
                  disabled={createBusy || login.trim().length === 0 || password.length === 0}
                  className="inline-flex items-center gap-2 rounded-xl bg-gradient-to-b from-[#e0c486] to-[#c9a45c] px-5 py-2 text-sm font-medium text-[#1a1407] transition hover:from-[#f0d59a] hover:to-[#d4af68] disabled:cursor-not-allowed disabled:opacity-45"
                >
                  {createBusy ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : mode === "claim" ? (
                    <Link2 className="size-4" />
                  ) : (
                    <UserPlus className="size-4" />
                  )}
                  {mode === "claim" ? "Привязать" : "Создать"}
                </button>
                <button
                  onClick={reset}
                  className="rounded-lg border border-[rgba(201,164,92,0.2)] px-4 py-2 text-sm text-[rgba(233,228,216,0.7)] transition hover:text-[#c9a45c]"
                >
                  Отмена
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
