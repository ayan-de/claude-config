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
          <div
            className={cn(
              "h-full opacity-[0.07] transition-all duration-500 relative bg-gradient-to-r",
              session5hPct >= 80
                ? "from-rose-500 to-red-600"
                : session5hPct >= 50
                ? "from-amber-500 to-orange-500"
                : "from-emerald-500 to-teal-500"
            )}
            style={{ width: `${Math.max(0, Math.min(100, session5hPct))}%` }}
          >
            {/* Shimmer fluid light element */}
            <div className="absolute inset-0 bg-gradient-to-r from-transparent via-white/45 to-transparent -translate-x-full animate-fluid-shimmer" />

            {/* Dual animated vertical waves along the leading edge */}
            <div className="absolute inset-y-0 right-0 w-3 overflow-hidden pointer-events-none">
              {/* Back wave */}
              <svg
                viewBox="0 0 20 200"
                className="absolute inset-y-0 right-0 h-[200%] w-full animate-wave-vertical-slow text-white/20 fill-current"
                preserveAspectRatio="none"
              >
                <path d="M0,0 C10,12.5 10,12.5 0,25 C10,37.5 10,37.5 0,50 C10,62.5 10,62.5 0,75 C10,87.5 10,87.5 0,100 C10,112.5 10,112.5 0,125 C10,137.5 10,137.5 0,150 C10,162.5 10,162.5 0,175 C10,187.5 10,187.5 0,200 L20,200 L20,0 Z" />
              </svg>
              {/* Front wave */}
              <svg
                viewBox="0 0 20 200"
                className="absolute inset-y-0 right-0 h-[200%] w-full animate-wave-vertical-fast text-white/40 fill-current -mr-[1px]"
                preserveAspectRatio="none"
              >
                <path d="M0,0 C12,12.5 12,12.5 0,25 C12,37.5 12,37.5 0,50 C12,62.5 12,62.5 0,75 C12,87.5 12,87.5 0,100 C12,112.5 12,112.5 0,125 C12,137.5 12,137.5 0,150 C12,162.5 12,162.5 0,175 C12,187.5 12,187.5 0,200 L20,200 L20,0 Z" />
              </svg>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}