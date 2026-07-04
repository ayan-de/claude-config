"use client";

import { CheckCircle2 } from "lucide-react";
import type { Provider } from "@/lib/types";
import { Badge } from "@/components/ui/badge";

export function ActiveBanner({ provider }: { provider: Provider }) {
  let host = provider.baseUrl;
  try {
    host = new URL(provider.baseUrl).host;
  } catch {
    // Invalid URL — fall back to the raw string. Doesn't crash the UI.
  }
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border border-emerald-500/20 bg-emerald-500/5 px-4 py-2.5">
      <div className="flex items-center gap-2.5">
        <CheckCircle2 className="size-4 text-emerald-400" />
        <span className="text-xs uppercase tracking-wider text-muted-foreground">
          Active
        </span>
        <span className="font-medium text-foreground">{provider.name}</span>
        <Badge variant="secondary" className="font-mono text-[10px]">
          {host}
        </Badge>
      </div>
      <span className="text-[10px] text-muted-foreground/70">
        updated {new Date(provider.updatedAt).toLocaleDateString()}
      </span>
    </div>
  );
}