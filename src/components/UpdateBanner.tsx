"use client";

import { AlertTriangle, X } from "lucide-react";
import { Button } from "@/components/ui/button";

interface Props {
  version: string;
  downloading: boolean;
  onInstall: () => void;
  onDismiss: () => void;
}

export function UpdateBanner({
  version,
  downloading,
  onInstall,
  onDismiss,
}: Props) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-2.5">
      <div className="flex items-center gap-2.5 text-amber-100">
        <AlertTriangle className="size-4 shrink-0 text-amber-300" />
        <div className="space-y-0.5">
          <p className="text-xs font-medium">Update available: v{version}</p>
          <p className="text-[10px] text-amber-200/70">
            A newer version is ready to install.
          </p>
        </div>
      </div>
      <div className="flex items-center gap-2 shrink-0">
        <Button size="sm" variant="outline" onClick={onInstall} disabled={downloading}>
          {downloading ? "Updating…" : "Update"}
        </Button>
        <button
          type="button"
          onClick={onDismiss}
          aria-label="Dismiss update notification"
          className="flex size-7 items-center justify-center rounded-md text-amber-200/70 transition hover:bg-amber-500/20 hover:text-amber-100 focus:outline-none focus:ring-1 focus:ring-amber-300/40"
        >
          <X className="size-3.5" />
        </button>
      </div>
    </div>
  );
}