"use client";

import { Loader2, Pencil, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ProviderLogo } from "@/components/ProviderLogo";
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
  // Determine clean display subtitle
  const getCleanSubtitle = () => {
    switch (provider.kind) {
      case "subscription":
        return provider.subscriptionLabel
          ? `Subscription (${provider.subscriptionLabel})`
          : "Subscription";
      case "console":
        return "Anthropic Console";
      case "bedrock":
        return provider.awsRegion ? `Bedrock (${provider.awsRegion})` : "Amazon Bedrock";
      case "vertex":
        return provider.vertexRegion ? `Vertex AI (${provider.vertexRegion})` : "Google Vertex AI";
      case "custom":
      default:
        return "Custom Relay";
    }
  };

  const subtitle = getCleanSubtitle();

  return (
    <div
      className={cn(
        "group relative cursor-pointer rounded-xl border py-2.5 px-3 transition-all duration-200 select-none flex items-center justify-between gap-3 bg-card/45",
        isActive
          ? "border-primary/40 bg-primary/5 shadow-sm"
          : isSelected
          ? "border-foreground/30 bg-card/90"
          : "border-border/60 hover:border-foreground/20 hover:bg-card/70",
      )}
      onClick={onSelect}
      role="button"
      aria-pressed={isActive ? "true" : "false"}
      aria-label={`Switch to ${provider.name}`}
    >
      {/* Left section: Logo + Name & Subtitle */}
      <div className="flex items-center gap-3 min-w-0 flex-1">
        {/* Logo Container */}
        <div className="relative size-8 rounded-lg border bg-muted/20 flex items-center justify-center shrink-0 overflow-hidden">
          <ProviderLogo
            svg={provider.logoSvg}
            size={20}
            className="rounded"
          />
        </div>

        {/* Text Details */}
        <div className="min-w-0 flex-1 flex flex-col justify-center">
          <p
            className={cn(
              "text-xs font-semibold truncate leading-none transition-colors",
              isActive ? "text-foreground" : "text-foreground/90",
            )}
          >
            {provider.name}
          </p>
          <p className="text-[10px] text-muted-foreground/80 truncate leading-none mt-1">
            {subtitle}
          </p>
        </div>
      </div>

      {/* Right section: Toggle Switch / Actions */}
      <div className="flex items-center gap-2 shrink-0">
        {/* Switch */}
        <div
          className="tauri-no-drag"
          onClick={(e) => {
            e.stopPropagation();
            if (!isLoading) onLoad();
          }}
        >
          <div
            className={cn(
              "w-[34px] h-[20px] rounded-full border relative transition-colors duration-150 flex items-center cursor-pointer",
              isActive
                ? "bg-primary/20 border-primary/30"
                : "bg-muted/40 border-border hover:bg-muted/60",
              isLoading && "opacity-80 cursor-wait"
            )}
          >
            <div
              className={cn(
                "absolute top-[2px] w-[14px] h-[14px] rounded-full transition-all duration-150 flex items-center justify-center",
                isActive
                  ? "left-[16px] bg-primary"
                  : "left-[2px] bg-background border border-muted-foreground/30",
              )}
            >
              {isLoading && (
                <Loader2 className="size-1.5 animate-spin text-primary" />
              )}
            </div>
          </div>
        </div>

        {/* Hover Actions overlay (floating toolbar to the left of the switch) */}
        <div className="absolute right-[54px] top-1/2 -translate-y-1/2 flex items-center border bg-card/95 dark:bg-card/98 shadow-md rounded-lg p-0.5 opacity-0 group-hover:opacity-100 transition-all duration-200 translate-x-1 group-hover:translate-x-0 pointer-events-none group-hover:pointer-events-auto">
          <Button
            size="sm"
            variant="ghost"
            onClick={(e) => {
              e.stopPropagation();
              onSelect();
            }}
            className="h-6 w-6 p-0 hover:bg-muted dark:hover:bg-muted/50 cursor-pointer"
            aria-label="Edit"
          >
            <Pencil className="size-3 text-muted-foreground hover:text-foreground" />
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={(e) => {
              e.stopPropagation();
              onDelete();
            }}
            className="h-6 w-6 p-0 hover:bg-destructive/10 cursor-pointer"
            aria-label="Delete"
          >
            <Trash2 className="size-3 text-muted-foreground hover:text-destructive" />
          </Button>
        </div>
      </div>
    </div>
  );
}