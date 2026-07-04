"use client";

import { Loader2, Pencil, Trash2 } from "lucide-react";
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
    host = new URL(provider.base_url).host;
  } catch {
    host = provider.base_url;
  }

  return (
    <div
      className={cn(
        "group relative cursor-pointer rounded-xl border bg-card/60 py-2.5 px-3 transition-all duration-200 select-none flex flex-col items-center gap-2",
        isActive
          ? "border-[#b1ada1]/40 bg-[#b1ada1]/5 shadow-[0_0_12px_rgba(177,173,161,0.03)]"
          : isSelected
          ? "border-foreground/30 bg-card/90"
          : "border-border hover:border-foreground/20 hover:bg-card/80",
      )}
      onClick={onSelect}
      role="button"
      aria-pressed={isActive ? "true" : "false"}
      aria-label={`Switch to ${provider.name}`}
    >
      {/* hover-only actions positioned absolutely in the top-right corner */}
      <div className="absolute top-1.5 right-1.5 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
        <Button
          size="sm"
          variant="ghost"
          onClick={(e) => {
            e.stopPropagation();
            onSelect();
          }}
          className="h-5 w-5 p-0 hover:bg-accent cursor-pointer"
          aria-label="Edit"
        >
          <Pencil className="size-2.5 text-muted-foreground hover:text-foreground" />
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
          className="h-5 w-5 p-0 hover:bg-destructive/10 cursor-pointer"
          aria-label="Delete"
        >
          <Trash2 className="size-2.5 text-muted-foreground hover:text-destructive" />
        </Button>
      </div>

      {/* Center: Circuit-breaker Toggle Switch */}
      <div
        className="tauri-no-drag"
        onClick={(e) => {
          e.stopPropagation();
          if (!isLoading) onLoad();
        }}
      >
        <div
          className={cn(
            "w-11 h-6 rounded-full border relative transition-colors duration-150 flex items-center cursor-pointer",
            isActive
              ? "bg-[#b1ada1]/20 border-[#b1ada1]/30"
              : "bg-muted/40 border-border hover:bg-muted/60",
            isLoading && "opacity-80 cursor-wait"
          )}
        >
          <div
            className={cn(
              "absolute top-[2.5px] w-[17px] h-[17px] rounded-full transition-all duration-150 flex items-center justify-center",
              isActive
                ? "left-[22.5px] bg-[#b1ada1] border border-[#b1ada1]"
                : "left-[2.5px] bg-background border border-muted-foreground/30",
            )}
          >
            {isLoading && (
              <Loader2 className="size-2 animate-spin text-[#b1ada1]" />
            )}
          </div>
        </div>
      </div>

      {/* Bottom: Provider details */}
      <div className="text-center min-w-0 w-full">
        <p
          className={cn(
            "text-xs font-semibold truncate leading-none transition-colors",
            isActive ? "text-foreground" : "text-muted-foreground",
          )}
        >
          {provider.name}
        </p>
        <p className="mt-1 truncate font-mono text-[9px] text-muted-foreground/75 leading-none">
          {host}
        </p>
      </div>
    </div>
  );
}