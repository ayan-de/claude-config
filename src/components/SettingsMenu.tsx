"use client";

import {
  Download,
  ExternalLink,
  FolderOpen,
  Settings as SettingsIcon,
  Upload,
} from "lucide-react";
import { Button } from "@/components/ui/button";
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
  onRevealAppDir: () => void;
  onRevealClaudeDir: () => void;
  onExport: (includeSecrets: boolean) => void;
  onImport: () => void;
}

export function SettingsMenu({
  appDataDir,
  claudeDir,
  onRevealAppDir,
  onRevealClaudeDir,
  onExport,
  onImport,
}: Props) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button variant="ghost" size="icon-sm" aria-label="Settings" />
        }
      >
        <SettingsIcon className="size-4" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-64">
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
      </DropdownMenuContent>
    </DropdownMenu>
  );
}