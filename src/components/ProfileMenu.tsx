import { useState } from "react";
import { User, LogOut, ChevronUp } from "lucide-react";
import type { LauncherUser } from "../lib/api";

/** Мини-профиль: чип с именем, по клику — выпадающее меню (email + выход). */
export function ProfileMenu({ me, onLogout }: { me: LauncherUser; onLogout: () => void }) {
  const [open, setOpen] = useState(false);
  const name = me.name ?? me.email ?? "Игрок";

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        className="inline-flex items-center gap-2 rounded-full border border-[rgba(201,164,92,0.25)] bg-white/[0.03] px-2.5 py-1 text-xs text-[rgba(233,228,216,0.85)] transition hover:border-[rgba(201,164,92,0.5)]"
      >
        <span className="grid size-5 place-items-center rounded-full bg-[rgba(201,164,92,0.18)] text-[#c9a45c]">
          <User className="size-3" />
        </span>
        <span className="max-w-[140px] truncate">{name}</span>
        <ChevronUp
          className={`size-3.5 text-[rgba(233,228,216,0.5)] transition ${open ? "" : "rotate-180"}`}
        />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div className="absolute bottom-full left-0 z-50 mb-2 w-60 overflow-hidden rounded-xl border border-[rgba(201,164,92,0.2)] bg-[#15130f] shadow-[0_18px_50px_-20px_rgba(0,0,0,0.8)]">
            <div className="border-b border-[rgba(201,164,92,0.12)] px-4 py-3">
              <div className="flex items-center gap-2.5">
                <span className="grid size-9 place-items-center rounded-full bg-[rgba(201,164,92,0.16)] text-[#c9a45c]">
                  <User className="size-4" />
                </span>
                <div className="min-w-0">
                  <div className="truncate text-sm text-[#e9e4d8]">{name}</div>
                  {me.email && (
                    <div className="truncate text-xs text-[rgba(233,228,216,0.5)]">{me.email}</div>
                  )}
                </div>
              </div>
            </div>
            <button
              onClick={() => {
                setOpen(false);
                onLogout();
              }}
              className="flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm text-[rgba(233,228,216,0.8)] transition hover:bg-[rgba(201,164,92,0.08)] hover:text-[#c9a45c]"
            >
              <LogOut className="size-4" /> Выйти
            </button>
          </div>
        </>
      )}
    </div>
  );
}
