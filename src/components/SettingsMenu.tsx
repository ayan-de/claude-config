"use client";

import {
  Download,
  ExternalLink,
  FolderOpen,
  RefreshCw,
  Settings as SettingsIcon,
  Upload,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

interface Props {
  appDataDir: string | null;
  claudeDir: string | null;
  updateAvailable: boolean;
  updateError: string | null;
  onRevealAppDir: () => void;
  onRevealClaudeDir: () => void;
  onExport: (includeSecrets: boolean) => void;
  onImport: () => void;
  onCheckForUpdates: () => void;
  dangerousMode: boolean | null;
  onToggleDangerousMode: () => void;
}

export function SettingsMenu({
  appDataDir,
  claudeDir,
  updateAvailable,
  updateError,
  onRevealAppDir,
  onRevealClaudeDir,
  onExport,
  onImport,
  onCheckForUpdates,
  dangerousMode,
  onToggleDangerousMode,
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
          <DropdownMenuLabel>Updates</DropdownMenuLabel>
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