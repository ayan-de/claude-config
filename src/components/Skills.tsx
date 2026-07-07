"use client";

import { useState } from "react";
import {
  ArrowLeft,
  ChevronDown,
  ChevronRight,
  Folder,
  Loader2,
  Plug,
  RefreshCw,
  Sparkles,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { useSkills } from "@/hooks/useSkills";
import type { SkillSummary } from "@/lib/types";
import type {
  GlobalTabProps,
  SidebarTabButtonProps,
} from "@/data/globalTabs";
import { cn } from "@/lib/utils";

// Sidebar entry — same visual shape as MarketplaceSidebarButton (icon +
// label, pill highlight when active). No "+ Add" affordance because the
// add flow is "drop a SKILL.md into the right directory" — there's no
// dialog to open from here.
export function SkillsSidebarButton({
  active,
  onSelect,
}: SidebarTabButtonProps) {
  return (
    <button
      onClick={onSelect}
      className={cn(
        "w-full flex items-center gap-2 px-3 py-2 rounded-lg border text-left text-xs font-medium transition-all cursor-pointer group",
        active
          ? "bg-primary/10 border-primary/20 text-primary shadow-2xs"
          : "bg-card/50 border-border/60 text-muted-foreground hover:bg-card hover:border-foreground/20 hover:text-foreground",
      )}
    >
      <Sparkles
        className={cn(
          "size-3.5 shrink-0",
          active
            ? "text-primary"
            : "text-muted-foreground group-hover:text-foreground",
        )}
      />
      <span className="flex-1 truncate">Skills</span>
    </button>
  );
}

export function SkillsView({ onClose }: GlobalTabProps) {
  const { skills, loading, refresh } = useSkills();

  // skills === null means first load still in flight.
  const initialLoad = skills === null && loading;

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5">
          <Button size="sm" variant="ghost" onClick={onClose}>
            <ArrowLeft className="size-3.5" />
          </Button>
          <Sparkles className="size-4 text-primary" />
          <div>
            <h2 className="text-sm font-semibold leading-none">Skills</h2>
            <p className="mt-1 text-[11px] text-muted-foreground">
              Reusable SKILL.md instructions Claude Code loads on demand.
            </p>
          </div>
        </div>
        <Button
          size="sm"
          variant="outline"
          onClick={() => void refresh()}
          disabled={loading}
          aria-label="Refresh skills list"
        >
          {loading ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <RefreshCw className="size-3.5" />
          )}
          Refresh
        </Button>
      </div>

      <UserSkillsSection
        skills={skills ?? []}
        initialLoad={initialLoad}
      />

      <PluginSkillsSection
        skills={skills ?? []}
        initialLoad={initialLoad}
      />
    </div>
  );
}

function UserSkillsSection({
  skills,
  initialLoad,
}: {
  skills: SkillSummary[];
  initialLoad: boolean;
}) {
  const rows = skills.filter((s) => s.source.kind === "user");

  if (initialLoad) {
    return (
      <SectionPanel>
        <PanelHeader icon={<Folder className="size-3.5" />} label="User skills" />
        <div className="flex items-center justify-center gap-2 p-6 text-xs text-muted-foreground">
          <Loader2 className="size-3.5 animate-spin" />
          Loading…
        </div>
      </SectionPanel>
    );
  }

  if (rows.length === 0) {
    return (
      <SectionPanel>
        <PanelHeader icon={<Folder className="size-3.5" />} label="User skills" />
        <EmptyHint
          title="No user skills yet"
          body="Drop a SKILL.md into ~/.claude/skills/<name>/SKILL.md to add one."
        />
      </SectionPanel>
    );
  }

  return (
    <SectionPanel>
      <PanelHeader
        icon={<Folder className="size-3.5" />}
        label="User skills"
        count={rows.length}
      />
      <ul className="divide-y divide-border/40">
        {rows.map((s) => (
          <SkillRow key={`${s.source.kind}:${s.path}`} skill={s} />
        ))}
      </ul>
    </SectionPanel>
  );
}

function PluginSkillsSection({
  skills,
  initialLoad,
}: {
  skills: SkillSummary[];
  initialLoad: boolean;
}) {
  // Group plugin skills by their `<plugin>@<marketplace>` key.
  const groups = group_plugin_skills(skills);

  if (initialLoad) return null;

  if (groups.length === 0) {
    return (
      <SectionPanel>
        <PanelHeader icon={<Plug className="size-3.5" />} label="Plugin skills" />
        <EmptyHint
          title="No plugin skills installed"
          body="Install a plugin from a marketplace to see its bundled skills here."
        />
      </SectionPanel>
    );
  }

  return (
    <SectionPanel>
      <PanelHeader icon={<Plug className="size-3.5" />} label="Plugin skills" />
      <div className="divide-y divide-border/40">
        {groups.map(({ key, rows }) => (
          <PluginGroup key={key} groupKey={key} rows={rows} />
        ))}
      </div>
    </SectionPanel>
  );
}

function PluginGroup({
  groupKey,
  rows,
}: {
  groupKey: string;
  rows: SkillSummary[];
}) {
  const [open, setOpen] = useState(true);
  const enabledCount = rows.filter((r) => r.enabled).length;
  const allDisabled = enabledCount === 0;

  return (
    <div className="flex flex-col">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-controls={`plugin-skills-panel-${groupKey}`}
        className="group flex w-full items-center justify-between gap-3 px-4 py-3 text-left cursor-pointer hover:bg-card/60 transition-colors"
      >
        <div className="flex items-center gap-2 truncate">
          {open ? (
            <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
          ) : (
            <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
          )}
          <h3 className="truncate text-sm font-semibold font-mono">
            {groupKey}
          </h3>
        </div>
        <span
          className={cn(
            "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider",
            allDisabled
              ? "bg-muted/60 text-muted-foreground"
              : "bg-primary/10 text-primary",
          )}
          title={
            allDisabled
              ? `${rows.length} skill${rows.length === 1 ? "" : "s"}, plugin disabled`
              : `${enabledCount} of ${rows.length} enabled`
          }
        >
          {allDisabled
            ? `${rows.length} (disabled)`
            : `${enabledCount} / ${rows.length} enabled`}
        </span>
      </button>

      {open && (
        <ul
          id={`plugin-skills-panel-${groupKey}`}
          className="divide-y divide-border/40 bg-background/30"
        >
          {rows.map((s) => (
            <SkillRow key={`${s.source.kind}:${s.path}`} skill={s} />
          ))}
        </ul>
      )}
    </div>
  );
}

function SkillRow({ skill }: { skill: SkillSummary }) {
  return (
    <li className="flex flex-col gap-1 px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 truncate">
          <h4 className="truncate text-sm font-medium">{skill.name}</h4>
          {skill.source.kind === "plugin" && (
            <span className="truncate text-[10px] text-muted-foreground font-mono">
              v{skill.source.version}
            </span>
          )}
        </div>
        {skill.source.kind === "plugin" && !skill.enabled && (
          <span className="shrink-0 rounded-full bg-muted/60 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
            Disabled
          </span>
        )}
      </div>
      {skill.description ? (
        <p className="text-xs text-muted-foreground/90 line-clamp-2">
          {skill.description}
        </p>
      ) : (
        <p className="text-[11px] italic text-muted-foreground/60">
          No description provided.
        </p>
      )}
      <p
        className="truncate font-mono text-[10px] text-muted-foreground/50"
        title={skill.path}
      >
        {skill.path}
      </p>
    </li>
  );
}

function SectionPanel({ children }: { children: React.ReactNode }) {
  return (
    <div className="overflow-hidden rounded-xl border bg-card/45">
      {children}
    </div>
  );
}

function PanelHeader({
  icon,
  label,
  count,
}: {
  icon: React.ReactNode;
  label: string;
  count?: number;
}) {
  return (
    <div className="flex items-center justify-between gap-2 border-b border-border/40 px-4 py-2.5">
      <div className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
        {icon}
        {label}
      </div>
      {typeof count === "number" && (
        <span className="text-[10px] tabular-nums text-muted-foreground/60">
          ({count})
        </span>
      )}
    </div>
  );
}

function EmptyHint({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex flex-col items-center gap-2 p-6 text-center">
      <p className="text-xs font-medium">{title}</p>
      <p className="text-[11px] text-muted-foreground">{body}</p>
    </div>
  );
}

/**
 * Group plugin skills by `<plugin>@<marketplace>`. Plugin skills are
 * already sorted alphabetically within their group by the backend, and
 * groups are sorted by marketplace/plugin. We preserve insertion order so
 * the UI doesn't shuffle on re-render.
 */
function group_plugin_skills(
  skills: SkillSummary[],
): Array<{ key: string; rows: SkillSummary[] }> {
  const order: string[] = [];
  const map = new Map<string, SkillSummary[]>();
  for (const s of skills) {
    if (s.source.kind !== "plugin") continue;
    const key = `${s.source.plugin}@${s.source.marketplace}`;
    if (!map.has(key)) {
      map.set(key, []);
      order.push(key);
    }
    map.get(key)!.push(s);
  }
  return order.map((key) => ({ key, rows: map.get(key)! }));
}