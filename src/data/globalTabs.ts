"use client";

import { FileText, PlugZap, Sparkles, Store, type LucideIcon } from "lucide-react";
import type { ComponentType } from "react";

import {
  ClaudeMdEditor,
  ClaudeMdSidebarButton,
} from "@/components/ClaudeMdEditor";
import {
  MarketplaceSidebarButton,
  MarketplaceView,
} from "@/components/Marketplace";
import {
  McpSidebarButton,
  McpView,
} from "@/components/Mcp";
import {
  SkillsSidebarButton,
  SkillsView,
} from "@/components/Skills";

export interface GlobalTabProps {
  /** Called when the tab wants to close itself (e.g. Cancel / back button). */
  onClose: () => void;
}

/**
 * Standard props passed to every tab's sidebar entry. Each tab owns its own
 * SidebarButton because visibility / "exists? → label" logic is tab-specific
 * (CLAUDE.md has the "+ Add" affordance; settings.json or hooks probably won't).
 * Open/Closed: adding a new tab = append one entry below. Zero page.tsx edits.
 */
export interface SidebarTabButtonProps {
  active: boolean;
  onSelect: () => void;
}

export interface GlobalTab {
  id: string;
  label: string;
  icon: LucideIcon;
  /** Hover text shown in the workspace info icon — explains what this tab is for. */
  tooltip: string;
  /** Sidebar entry. Each tab implements its own because existence indicators vary. */
  SidebarButton: ComponentType<SidebarTabButtonProps>;
  /** Main-pane view rendered when this tab is the active one. */
  Component: ComponentType<GlobalTabProps>;
}

/**
 * Static registry of "global config" tabs (CLAUDE.md today, more later).
 * Adding a tab = append one entry, no other code changes anywhere.
 */
export const GLOBAL_TABS: readonly GlobalTab[] = [
  {
    id: "claude-md",
    label: "CLAUDE.md",
    icon: FileText,
    tooltip:
      "Give global instructions to Claude Code from here. These rules apply across all repositories you run Claude Code in.",
    SidebarButton: ClaudeMdSidebarButton,
    Component: ClaudeMdEditor,
  },
  {
    id: "marketplace",
    label: "Marketplace",
    icon: Store,
    tooltip:
      "Browse registries of plugins, skills, and commands contributed for Claude Code.",
    SidebarButton: MarketplaceSidebarButton,
    Component: MarketplaceView,
  },
  {
    id: "skills",
    label: "Skills",
    icon: Sparkles,
    tooltip:
      "Reusable SKILL.md instructions loaded by Claude Code on demand — your own plus those bundled with installed plugins.",
    SidebarButton: SkillsSidebarButton,
    Component: SkillsView,
  },
  {
    id: "mcp",
    label: "MCP",
    icon: PlugZap,
    tooltip:
      "Browse MCP servers Claude Code connects to — globally configured in ~/.claude.json.",
    SidebarButton: McpSidebarButton,
    Component: McpView,
  },
];

export type GlobalTabId = (typeof GLOBAL_TABS)[number]["id"];
