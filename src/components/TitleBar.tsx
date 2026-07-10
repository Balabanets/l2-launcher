import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, X } from "lucide-react";

export function TitleBar() {
  const win = getCurrentWindow();
  return (
    <div className="drag flex h-11 items-center justify-between px-4 border-b border-[rgba(201,164,92,0.12)]">
      {/* Бренд как в шапке сайта: эмблема-кристалл + серебристый wordmark LUNARGENT */}
      <div className="flex items-center gap-2.5">
        <img src="/brand/crystal-bolt.webp" alt="" aria-hidden className="h-9 w-auto object-contain" />
        <img src="/brand/wordmark.webp" alt="LUNARGENT" className="h-[1.3rem] w-auto object-contain" />
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
