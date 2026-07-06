"use client";

import { useCallback, useState } from "react";

import type { GlobalTabId } from "@/data/globalTabs";

export function useGlobalPanel() {
  const [activeTabId, setActiveTabId] = useState<GlobalTabId | null>(null);

  const openTab = useCallback((id: GlobalTabId) => setActiveTabId(id), []);
  const close = useCallback(() => setActiveTabId(null), []);

  return {
    activeTabId,
    isOpen: activeTabId !== null,
    openTab,
    close,
  };
}
