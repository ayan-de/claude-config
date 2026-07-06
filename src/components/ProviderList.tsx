"use client";

import * as React from "react";
import { useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Info,
  PanelLeftClose,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

export interface SidebarSection {
  /** Stable id — drives React key + expansion state. */
  id: string;
  label: string;
  icon: LucideIcon;
  /** Optional info-tooltip text shown in the section header. */
  headerTooltip?: string;
  /** Whether the section starts expanded. Defaults to true. */
  defaultExpanded?: boolean;
  /** Anything you want rendered inside when the section is open. */
  content: React.ReactNode;
}

interface Props {
  sections: SidebarSection[];
  collapsed: boolean;
  onToggleCollapse: () => void;
}

/**
 * Generic left-side accordion. Sections are passed in by the page; ProviderList
 * only knows about expansion state and header chrome, never about what's
 * inside. Adding a section = one entry in `sections` at the call site.
 */
export function ProviderList({ sections, collapsed, onToggleCollapse }: Props) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>(() =>
    Object.fromEntries(
      sections.map((s) => [s.id, s.defaultExpanded ?? true]),
    ),
  );

  return (
    <aside
      className={cn(
        "flex h-full flex-col border-r bg-card/30 transition-all duration-300 ease-in-out overflow-hidden shrink-0",
        collapsed ? "w-0 border-r-0" : "w-72",
      )}
    >
      <div className="flex h-11 shrink-0 items-center justify-between border-b px-4 select-none">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          Workspace
        </h2>
        <Button
          size="sm"
          variant="ghost"
          className="h-7 w-7 p-0 text-muted-foreground hover:text-foreground cursor-pointer"
          onClick={onToggleCollapse}
          title="Collapse sidebar"
        >
          <PanelLeftClose className="size-3.5" />
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto divide-y divide-border/40 select-none">
        {sections.map((s) => {
          const isOpen = expanded[s.id] ?? true;
          const Icon = s.icon;
          return (
            <div key={s.id} className="flex flex-col">
              <div className="flex items-center justify-between px-3 py-2 bg-muted/5 hover:bg-muted/10 transition-colors">
                <button
                  onClick={() =>
                    setExpanded((prev) => ({ ...prev, [s.id]: !isOpen }))
                  }
                  className="flex-1 flex items-center gap-1.5 text-left cursor-pointer text-xs font-medium text-muted-foreground hover:text-foreground transition-colors"
                >
                  {isOpen ? (
                    <ChevronDown className="size-3.5 text-muted-foreground/60" />
                  ) : (
                    <ChevronRight className="size-3.5 text-muted-foreground/60" />
                  )}
                  <Icon className="size-3.5 text-muted-foreground/80" />
                  <span className="uppercase tracking-wider">{s.label}</span>
                </button>
                {s.headerTooltip && (
                  <TooltipProvider>
                    <Tooltip>
                      <TooltipTrigger className="text-muted-foreground/60 hover:text-foreground cursor-help p-1">
                        <Info className="size-3.5" />
                      </TooltipTrigger>
                      <TooltipContent side="right">
                        {s.headerTooltip}
                      </TooltipContent>
                    </Tooltip>
                  </TooltipProvider>
                )}
              </div>
              {isOpen && <div className="p-3">{s.content}</div>}
            </div>
          );
        })}
      </div>
    </aside>
  );
}
