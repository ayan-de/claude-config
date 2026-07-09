"use client";

import {
  BarChart3,
  FileText,
  History,
  PlugZap,
  Sparkles,
  Store,
  type LucideIcon,
} from "lucide-react";
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
import { SessionsSidebarButton, SessionsView } from "@/components/Sessions";
import {
  SkillsSidebarButton,
  SkillsView,
} from "@/components/Skills";
import { UsageSidebarButton, UsageView } from "@/components/Usage";

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
  /**
   * Sidebar entry. Each tab implements its own because existence
   * indicators vary. Required by the type but the Sidebar loop skips
   * tabs whose `hideInSidebar` is true, in which case the trigger is
   * mounted elsewhere (typically the TitleBar).
   */
  SidebarButton: ComponentType<SidebarTabButtonProps>;
  /**
   * When true, the Sidebar's Global Config section will not render
   * this tab's button. Use this when the trigger lives elsewhere
   * (e.g. TitleBar) but the tab still owns a main-pane view.
   */
  hideInSidebar?: boolean;
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
  {
    id: "sessions",
    label: "Sessions",
    icon: History,
    tooltip:
      "Claude Code conversation sessions stored on this PC. Click a row to read the transcript.",
    SidebarButton: SessionsSidebarButton,
    Component: SessionsView,
  },
  {
    id: "usage",
    label: "Usage",
    icon: BarChart3,
    tooltip:
      "See the latest cached usage snapshot across every provider with a configured tracker.",
    SidebarButton: UsageSidebarButton,
    Component: UsageView,
  },
];

export type GlobalTabId = (typeof GLOBAL_TABS)[number]["id"];
