"use client";

import {
  Download,
  ExternalLink,
  FolderOpen,
  RefreshCw,
  Settings as SettingsIcon,
  Upload,
  Clock,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
} from "@/components/ui/dropdown-menu";

interface Props {
  appDataDir: string | null;
  claudeDir: string | null;
  updateAvailable: boolean;
  updateVersion: string | null;
  updateDownloading: boolean;
  updateError: string | null;
  onRevealAppDir: () => void;
  onRevealClaudeDir: () => void;
  onExport: (includeSecrets: boolean) => void;
  onImport: () => void;
  onCheckForUpdates: () => void;
  onInstallUpdate: () => void;
  dangerousMode: boolean | null;
  onToggleDangerousMode: () => void;
  trackerRefreshInterval: number;
  onTrackerRefreshIntervalChange: (interval: number) => void;
}

export function SettingsMenu({
  appDataDir,
  claudeDir,
  updateAvailable,
  updateVersion,
  updateDownloading,
  updateError,
  onRevealAppDir,
  onRevealClaudeDir,
  onExport,
  onImport,
  onCheckForUpdates,
  onInstallUpdate,
  dangerousMode,
  onToggleDangerousMode,
  trackerRefreshInterval,
  onTrackerRefreshIntervalChange,
}: Props) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Settings"
            className="relative"
          />
        }
      >
        <SettingsIcon className="size-4" />
        {updateAvailable && (
          <span
            aria-hidden
            className="absolute right-1 top-1 size-2 rounded-full bg-red-500 ring-2 ring-card/30"
          />
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-70">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Locations</DropdownMenuLabel>
          <DropdownMenuItem onClick={onRevealClaudeDir}>
            <ExternalLink />
            <span className="flex-1">Claude config dir</span>
            <span className="ml-2 truncate font-mono text-[10px] text-muted-foreground">
              {claudeDir ? `~${claudeDir.split("/").pop()}/` : "—"}
            </span>
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onRevealAppDir}>
            <FolderOpen />
            <span className="flex-1">App data dir</span>
            <span className="ml-2 truncate font-mono text-[10px] text-muted-foreground">
              {appDataDir ? `…${appDataDir.slice(-20)}` : "—"}
            </span>
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuLabel>Backup</DropdownMenuLabel>
          <DropdownMenuItem onClick={() => onExport(false)}>
            <Download />
            Export providers (no secrets)
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => onExport(true)}>
            <Download />
            Export with secrets
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onImport}>
            <Upload />
            Import providers
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuLabel>Safety</DropdownMenuLabel>
          <DropdownMenuItem
            onClick={(e) => {
              e.preventDefault();
              onToggleDangerousMode();
            }}
            disabled={dangerousMode === null}
          >
            <span className="flex-1">Dangerous mode</span>
            <Switch
              checked={dangerousMode === true}
              onCheckedChange={() => onToggleDangerousMode()}
              disabled={dangerousMode === null}
              // Stop the parent item from re-firing the click handler.
              onClick={(e: React.MouseEvent) => e.stopPropagation()}
            />
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuLabel>Tracker Polling</DropdownMenuLabel>
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <Clock className="size-4" />
              <span className="flex-1">Interval</span>
              <span className="ml-2 text-[10px] text-muted-foreground">
                {trackerRefreshInterval === 60000 ? "1 min" : trackerRefreshInterval === 300000 ? "5 min" : "Paused"}
              </span>
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent className="w-40">
              <DropdownMenuRadioGroup
                value={trackerRefreshInterval.toString()}
                onValueChange={(val) => onTrackerRefreshIntervalChange(parseInt(val, 10))}
              >
                <DropdownMenuRadioItem value="60000">1 minute</DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="300000">5 minutes</DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="0">Paused</DropdownMenuRadioItem>
              </DropdownMenuRadioGroup>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuLabel>Updates</DropdownMenuLabel>
          {updateAvailable && (
            <DropdownMenuItem
              onClick={(e) => {
                e.preventDefault();
                onInstallUpdate();
              }}
              disabled={updateDownloading}
              className="text-emerald-500 focus:bg-emerald-500/10 focus:text-emerald-500 dark:focus:bg-emerald-500/20"
            >
              <Download className={cn("size-4", updateDownloading ? "animate-pulse" : "animate-bounce")} />
              <span className="flex-1 font-semibold">
                {updateDownloading ? "Downloading update..." : `Install update (v${updateVersion})`}
              </span>
            </DropdownMenuItem>
          )}
          <DropdownMenuItem onClick={onCheckForUpdates}>
            <RefreshCw />
            <span className="flex-1">Check for updates</span>
            {updateAvailable && (
              <span
                className="ml-2 size-2 shrink-0 rounded-full bg-red-500"
                aria-label="update available"
              />
            )}
            {!updateAvailable && updateError && (
              <span
                className="ml-2 size-2 shrink-0 rounded-full bg-amber-500"
                aria-label="update check failed"
                title={updateError}
              />
            )}
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}