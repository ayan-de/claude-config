"use client";

import { Loader2, Pencil, Play, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { Provider } from "@/lib/types";

interface Props {
  provider: Provider;
  isActive: boolean;
  isSelected: boolean;
  isLoading: boolean;
  onSelect: () => void;
  onLoad: () => void;
  onDelete: () => void;
}

export function ProviderCard({
  provider,
  isActive,
  isSelected,
  isLoading,
  onSelect,
  onLoad,
  onDelete,
}: Props) {
  let host = "";
  try {
    host = new URL(provider.baseUrl).host;
  } catch {
    host = provider.baseUrl;
  }
  return (
    <div
      className={cn(
        "group relative cursor-pointer rounded-lg border bg-card p-3 transition-colors",
        isSelected
          ? "border-foreground/30 bg-card/80 ring-1 ring-foreground/10"
          : "border-border hover:border-foreground/20",
      )}
      onClick={onSelect}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span
              className={cn(
                "size-1.5 shrink-0 rounded-full",
                isActive ? "bg-emerald-400" : "bg-muted-foreground/30",
              )}
              title={isActive ? "Active" : "Inactive"}
            />
            <span className="truncate text-sm font-medium">
              {provider.name}
            </span>
          </div>
          <p className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">
            {host}
          </p>
        </div>
      </div>

      <div
        className={cn(
          "mt-3 flex items-center gap-1.5 transition-opacity",
          isSelected ? "opacity-100" : "opacity-0 group-hover:opacity-100",
        )}
      >
        <Button
          size="sm"
          variant="default"
          onClick={(e) => {
            e.stopPropagation();
            onLoad();
          }}
          disabled={isLoading}
          className="h-7 flex-1 px-2 text-xs"
        >
          {isLoading ? (
            <Loader2 className="size-3 animate-spin" />
          ) : (
            <Play className="size-3" />
          )}
          Load
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={(e) => {
            e.stopPropagation();
            onSelect();
          }}
          className="h-7 px-2"
          aria-label="Edit"
        >
          <Pencil className="size-3" />
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
          className="h-7 px-2 hover:bg-destructive/10 hover:text-destructive"
          aria-label="Delete"
        >
          <Trash2 className="size-3" />
        </Button>
      </div>
    </div>
  );
}