"use client";

import { Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ProviderCard } from "./ProviderCard";
import type { Provider } from "@/lib/types";

interface Props {
  providers: Provider[];
  activeProviderId: string | null;
  selectedId: string | null;
  loadingId: string | null;
  onSelect: (id: string) => void;
  onLoad: (id: string) => void;
  onDelete: (id: string) => void;
  onNew: () => void;
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
}: Props) {
  return (
    <aside className="flex h-full w-72 shrink-0 flex-col border-r bg-card/30">
      <div className="flex items-center justify-between px-4 py-3">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          Providers ({providers.length})
        </h2>
        <Button size="sm" variant="ghost" onClick={onNew} className="h-7 px-2">
          <Plus className="size-3.5" />
          New
        </Button>
      </div>
      <div className="flex-1 space-y-2 overflow-y-auto px-3 pb-3">
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