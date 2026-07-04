"use client";

import { Plus, PanelLeftClose } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ProviderCard } from "./ProviderCard";
import type { Provider } from "@/lib/types";
import { cn } from "@/lib/utils";

interface Props {
  providers: Provider[];
  activeProviderId: string | null;
  selectedId: string | null;
  loadingId: string | null;
  onSelect: (id: string) => void;
  onLoad: (id: string) => void;
  onDelete: (id: string) => void;
  onNew: () => void;
  collapsed: boolean;
  onToggleCollapse: () => void;
}

export function ProviderList({
  providers,
  activeProviderId,
  selectedId,
  loadingId,
  onSelect,
  onLoad,
  onDelete,
  onNew,
  collapsed,
  onToggleCollapse,
}: Props) {
  return (
    <aside
      className={cn(
        "flex h-full flex-col border-r bg-card/30 transition-all duration-300 ease-in-out overflow-hidden shrink-0",
        collapsed ? "w-0 border-r-0" : "w-72"
      )}
    >
      {/* Sidebar Header - located directly under TitleBar */}
      <div className="flex h-11 shrink-0 items-center justify-between border-b px-4 select-none">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          Providers ({providers.length})
        </h2>
        <div className="flex items-center gap-1.5">
          <Button size="sm" variant="ghost" onClick={onNew} className="h-7 px-2">
            <Plus className="size-3.5" />
            New
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0 text-muted-foreground hover:text-foreground"
            onClick={onToggleCollapse}
            title="Collapse sidebar"
          >
            <PanelLeftClose className="size-3.5" />
          </Button>
        </div>
      </div>

      <div className="flex-1 space-y-2 overflow-y-auto px-3 py-3">
        {providers.length === 0 ? (
          <div className="rounded-lg border border-dashed p-6 text-center">
            <p className="text-xs text-muted-foreground">No providers yet</p>
            <Button
              size="sm"
              variant="outline"
              onClick={onNew}
              className="mt-3"
            >
              <Plus className="size-3.5" />
              Add your first
            </Button>
          </div>
        ) : (
          providers.map((p) => (
            <ProviderCard
              key={p.id}
              provider={p}
              isActive={p.id === activeProviderId}
              isSelected={p.id === selectedId}
              isLoading={p.id === loadingId}
              onSelect={() => onSelect(p.id)}
              onLoad={() => onLoad(p.id)}
              onDelete={() => onDelete(p.id)}
            />
          ))
        )}
      </div>
    </aside>
  );
}