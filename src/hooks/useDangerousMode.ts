"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

import { getDangerousMode, setDangerousMode } from "@/lib/api";

const ACK_KEY = "claude-config.dangerous-mode-ack";

export function useDangerousMode() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    getDangerousMode()
      .then((v) => {
        if (!cancelled) setEnabled(v);
      })
      .catch((e) => {
        if (!cancelled) {
          setEnabled(false);
          toast.error(`Could not read dangerous-mode state: ${e.message}`);
        }
      })
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const apply = useCallback(
    async (next: boolean) => {
      const prev = enabled;
      setEnabled(next); // optimistic
      try {
        await setDangerousMode(next);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setEnabled(prev); // rollback
        toast.error(`Could not save dangerous-mode state: ${msg}`);
      }
    },
    [enabled],
  );

  const toggle = useCallback(async () => {
    if (enabled === true) {
      // OFF — no confirmation needed.
      await apply(false);
      return;
    }
    // OFF → ON: gate on first-time acknowledgement.
    if (typeof window === "undefined") return;
    const acked = window.localStorage.getItem(ACK_KEY) === "1";
    if (acked) {
      await apply(true);
    } else {
      setConfirmOpen(true);
    }
  }, [enabled, apply]);

  const confirm = useCallback(async () => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(ACK_KEY, "1");
    }
    setConfirmOpen(false);
    await apply(true);
  }, [apply]);

  const dismissConfirm = useCallback(() => {
    setConfirmOpen(false);
  }, []);

  return {
    enabled,
    loaded,
    confirmOpen,
    toggle,
    confirm,
    dismissConfirm,
  };
}
