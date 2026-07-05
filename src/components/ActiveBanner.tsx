"use client";

import { CheckCircle2 } from "lucide-react";
import type { Provider } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { providerSubtitle } from "@/lib/utils-app";

export function ActiveBanner({ provider }: { provider: Provider }) {
  const subtitle = providerSubtitle(provider);
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border border-emerald-500/20 bg-emerald-500/5 px-4 py-2.5">
      <div className="flex items-center gap-2.5">
        <CheckCircle2 className="size-4 text-emerald-400" />
        <span className="text-xs uppercase tracking-wider text-muted-foreground">
          Active
        </span>
        <span className="font-medium text-foreground">{provider.name}</span>
        <Badge variant="secondary" className="font-mono text-[10px]">
          {subtitle}
        </Badge>
      </div>
      <span className="text-[10px] text-muted-foreground/70">
        updated {new Date(provider.updated_at).toLocaleDateString()}
      </span>
    </div>
  );
}