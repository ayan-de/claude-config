"use client";

import { ShieldAlert } from "lucide-react";
import type { KeyringStatus } from "@/lib/types";

export function KeyringWarning({ status }: { status: KeyringStatus | null }) {
  if (!status || status.status === "available") return null;
  return (
    <div className="flex items-start gap-3 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-amber-100">
      <ShieldAlert className="mt-0.5 size-4 shrink-0" />
      <div className="space-y-0.5 text-xs">
        <p className="font-medium">OS keyring is unavailable.</p>
        <p className="text-amber-200/80">
          Auth tokens cannot be persisted securely. {status.message} Provider
          saving is disabled until the keyring is available.
        </p>
      </div>
    </div>
  );
}