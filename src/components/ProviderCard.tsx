"use client";

import { Loader2 } from "lucide-react";
import { ProviderLogo } from "@/components/ProviderLogo";
import { cn } from "@/lib/utils";
import type { Provider } from "@/lib/types";

interface Props {
  provider: Provider;
  isActive: boolean;
  isSelected: boolean;
  isLoading: boolean;
  /** 5h-session usage percent (0..100). `null` hides the bar. */
  session5hPct: number | null;
  onSelect: () => void;
  onLoad: () => void;
  onDelete: () => void;
}

export function ProviderCard({
  provider,
  isActive,
  isSelected,
  isLoading,
  session5hPct,
  onSelect,
  onLoad,
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

      {/* Right section: Toggle Switch */}
      <div className="flex items-center gap-2 shrink-0">
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
      </div>

      {/* 5h session progress overlay — covers full card background with low opacity, fills left-to-right */}
      {session5hPct !== null && (
        <div
          className="pointer-events-none absolute inset-0 overflow-hidden rounded-xl"
          aria-hidden="true"
        >
          {/* Progress fill */}
          <div
            className={cn(
              "h-full transition-all duration-500 bg-gradient-to-r",
              session5hPct >= 80
                ? "opacity-[0.07] from-rose-500 to-red-600"
                : session5hPct >= 50
                ? "opacity-[0.07] from-amber-500 to-orange-500"
                : "opacity-20 from-primary to-primary/80"
            )}
            style={{ width: `${Math.max(0, Math.min(100, session5hPct))}%` }}
          />
          {/* Edge glowing wave shimmer line */}
          {session5hPct > 0 && session5hPct < 100 && (
            <div
              className={cn(
                "absolute top-0 bottom-0 w-[2px] transition-all duration-500 bg-gradient-to-b from-transparent via-current to-transparent bg-[size:100%_200%] animate-edge-shimmer shadow-[0_0_8px_currentColor]",
                session5hPct >= 80
                  ? "text-rose-500/80"
                  : session5hPct >= 50
                  ? "text-amber-500/80"
                  : "text-primary/80"
              )}
              style={{
                left: `calc(${Math.max(0, Math.min(100, session5hPct))}% - 1px)`,
              }}
            />
          )}
        </div>
      )}
    </div>
  );
}