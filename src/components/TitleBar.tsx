import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, X } from "lucide-react";
import { Sigil } from "./Sigil";

export function TitleBar() {
  const win = getCurrentWindow();
  return (
    <div className="drag flex h-11 items-center justify-between px-4 border-b border-[rgba(201,164,92,0.12)]">
      <div className="flex items-center gap-2.5">
        <Sigil className="h-5 w-5" />
        <span className="flex items-baseline gap-1.5 leading-none">
          <span className="font-display text-sm font-semibold tracking-wider text-gold-gradient">L2</span>
          <span className="font-heading text-[0.7rem] tracking-[0.25em] text-[rgba(233,228,216,0.6)] uppercase">
            Interlude
          </span>
        </span>
      </div>
      <div className="no-drag flex items-center gap-1">
        <button
          onClick={() => win.minimize()}
          className="grid size-8 place-items-center rounded-md text-[rgba(233,228,216,0.6)] hover:bg-white/5 hover:text-gold transition-colors"
          aria-label="Свернуть"
        >
          <Minus className="size-4" />
        </button>
        <button
          onClick={() => win.close()}
          className="grid size-8 place-items-center rounded-md text-[rgba(233,228,216,0.6)] hover:bg-red-500/15 hover:text-red-400 transition-colors"
          aria-label="Закрыть"
        >
          <X className="size-4" />
        </button>
      </div>
    </div>
  );
}
