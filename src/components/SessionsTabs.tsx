"use client";

import { cn } from "@/lib/utils";

interface SessionsTabsProps {
  active: "local" | "remote";
  onChange: (next: "local" | "remote") => void;
}

export function SessionsTabs({ active, onChange }: SessionsTabsProps) {
  return (
    <div role="tablist" className="flex gap-1 rounded-md border bg-card/40 p-0.5">
      <button
        type="button"
        role="tab"
        aria-selected={active === "local"}
        onClick={() => onChange("local")}
        className={cn(
          "flex-1 rounded-sm px-3 py-1 text-xs transition-colors",
          active === "local"
            ? "border border-primary/20 bg-primary/10 text-primary"
            : "border border-transparent text-muted-foreground hover:bg-card"
        )}
      >
        Local
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={active === "remote"}
        onClick={() => onChange("remote")}
        className={cn(
          "flex-1 rounded-sm px-3 py-1 text-xs transition-colors",
          active === "remote"
            ? "border border-primary/20 bg-primary/10 text-primary"
            : "border border-transparent text-muted-foreground hover:bg-card"
        )}
      >
        Remote
      </button>
    </div>
  );
}
