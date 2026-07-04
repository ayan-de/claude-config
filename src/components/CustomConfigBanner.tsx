"use client";

import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";

interface Props {
  envKeys: string[];
  onSaveAs: () => void;
}

export function CustomConfigBanner({ envKeys, onSaveAs }: Props) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-2.5">
      <div className="flex items-center gap-2.5 text-amber-100">
        <AlertTriangle className="size-4 shrink-0 text-amber-300" />
        <div className="space-y-0.5">
          <p className="text-xs font-medium">Custom configuration</p>
          <p className="text-[10px] text-amber-200/70">
            {envKeys.length} env key{envKeys.length === 1 ? "" : "s"} in
            settings.json don&apos;t match any saved provider.
          </p>
        </div>
      </div>
      <Button size="sm" variant="outline" onClick={onSaveAs}>
        Save as new provider
      </Button>
    </div>
  );
}