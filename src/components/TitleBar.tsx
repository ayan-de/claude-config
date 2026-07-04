"use client";

import { Copy, Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

import { isMac, isTauri } from "@/lib/platform";

export function TitleBar({
  left,
  actions,
}: {
  left?: React.ReactNode;
  actions?: React.ReactNode;
}) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      const w = getCurrentWindow();
      const initial = await w.isMaximized();
      if (cancelled) return;
      setMaximized(initial);
      const off = await w.onResized(async () => {
        setMaximized(await w.isMaximized());
      });
      if (cancelled) {
        off();
      } else {
        unlisten = off;
      }
    })();
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  const onMin = () => {
    if (isTauri) void getCurrentWindow().minimize();
  };
  const onToggleMax = () => {
    if (isTauri) void getCurrentWindow().toggleMaximize();
  };
  const onClose = () => {
    if (isTauri) void getCurrentWindow().close();
  };

  const ctrlBtn =
    "flex size-7 items-center justify-center rounded-md transition shrink-0";

  return (
    <div className="flex h-11 w-full items-center justify-between border-b bg-card/30 pl-3 pr-1 select-none tauri-drag">
      <div className="flex items-center gap-3 min-w-0 flex-1">
        {isMac ? (
          <div className="tauri-no-drag flex items-center gap-1.5 shrink-0">
            <button
              type="button"
              onClick={onClose}
              aria-label="Close"
              className="size-3 rounded-full bg-red-500 transition hover:brightness-110 focus:outline-none focus:ring-1 focus:ring-foreground/20 shrink-0"
            />
            <button
              type="button"
              onClick={onMin}
              aria-label="Minimize"
              className="size-3 rounded-full bg-yellow-500 transition hover:brightness-110 focus:outline-none focus:ring-1 focus:ring-foreground/20 shrink-0"
            />
            <button
              type="button"
              onClick={onToggleMax}
              aria-label={maximized ? "Restore" : "Maximize"}
              className="size-3 rounded-full bg-green-500 transition hover:brightness-110 focus:outline-none focus:ring-1 focus:ring-foreground/20 shrink-0"
            />
          </div>
        ) : null}
        {left}
      </div>

      <div className="tauri-no-drag flex items-center gap-0.5 shrink-0">
        {actions}
        <button
          type="button"
          onClick={onMin}
          aria-label="Minimize"
          className={`${ctrlBtn} text-foreground/70 hover:bg-foreground/10 hover:text-foreground`}
        >
          <Minus className="size-3.5" />
        </button>
        <button
          type="button"
          onClick={onToggleMax}
          aria-label={maximized ? "Restore" : "Maximize"}
          className={`${ctrlBtn} text-foreground/70 hover:bg-foreground/10 hover:text-foreground`}
        >
          {maximized ? (
            <Copy className="size-3.5" />
          ) : (
            <Square className="size-3.5" />
          )}
        </button>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close"
          className={`${ctrlBtn} text-foreground/70 hover:bg-foreground/10 hover:text-foreground`}
        >
          <X className="size-3.5" />
        </button>
      </div>
    </div>
  );
}
