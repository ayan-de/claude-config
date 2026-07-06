"use client";

import { useState } from "react";
import {
  ArrowLeft,
  Loader2,
  Package,
  Plus,
  Store,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useMarketplaces } from "@/hooks/useMarketplaces";
import type {
  GlobalTabProps,
  SidebarTabButtonProps,
} from "@/data/globalTabs";
import { cn } from "@/lib/utils";

// Sidebar entry — matches the visual shape of ClaudeMdSidebarButton (icon +
// label, pill highlight when active). No "+ Add" affordance here because
// opening the tab is itself the affordance; the Add button lives inside
// the main pane so it has more space for the input dialog.
export function MarketplaceSidebarButton({
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
      <Store
        className={cn(
          "size-3.5 shrink-0",
          active
            ? "text-primary"
            : "text-muted-foreground group-hover:text-foreground",
        )}
      />
      <span className="flex-1 truncate">Marketplace</span>
    </button>
  );
}

export function MarketplaceView({ onClose }: GlobalTabProps) {
  const { marketplaces, loading, refresh } = useMarketplaces();
  const [addOpen, setAddOpen] = useState(false);

  // marketplaces === null means first load still in flight.
  const initialLoad =
    marketplaces === null && loading;

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5">
          <Store className="size-4 text-primary" />
          <div>
            <h2 className="text-sm font-semibold leading-none">
              Marketplace
            </h2>
            <p className="mt-1 text-[11px] text-muted-foreground">
              Sources for Claude Code plugins, skills, and commands.
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            variant="outline"
            onClick={() => void refresh()}
            disabled={loading}
            aria-label="Refresh marketplace list"
          >
            {loading ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Package className="size-3.5" />
            )}
            Refresh
          </Button>
          <Button
            size="sm"
            onClick={() => setAddOpen(true)}
            aria-label="Add Marketplace"
          >
            <Plus className="size-3.5" />
            Add Marketplace
          </Button>
          <Button size="sm" variant="ghost" onClick={onClose}>
            <ArrowLeft className="size-3.5" />
            Back
          </Button>
        </div>
      </div>

      <MarketplaceList
        marketplaces={marketplaces}
        initialLoad={initialLoad}
      />

      {addOpen && (
        <MarketplaceAddDialog onOpenChange={(open) => setAddOpen(open)} />
      )}
    </div>
  );
}

function MarketplaceList({
  marketplaces,
  initialLoad,
}: {
  marketplaces: import("@/lib/types").MarketplaceSummary[] | null;
  initialLoad: boolean;
}) {
  if (initialLoad) {
    return (
      <div className="flex items-center justify-center gap-2 rounded-xl border bg-card/45 p-8 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" />
        Loading marketplaces…
      </div>
    );
  }

  const rows = marketplaces ?? [];
  if (rows.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed bg-card/30 p-8 text-center">
        <Store className="size-5 text-muted-foreground/60" />
        <div className="space-y-1">
          <p className="text-sm font-medium">No marketplaces registered</p>
          <p className="text-[11px] text-muted-foreground">
            Add one to discover plugins, skills, and commands.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col divide-y divide-border/40 overflow-hidden rounded-xl border bg-card/45">
      {rows.map((m) => (
        <div key={m.name} className="flex flex-col gap-1 p-4">
          <div className="flex items-baseline justify-between gap-3">
            <div className="flex items-baseline gap-2 truncate">
              <h3 className="truncate text-sm font-semibold">{m.name}</h3>
              {m.owner && (
                <span className="truncate text-[10px] text-muted-foreground">
                  by {m.owner}
                </span>
              )}
            </div>
            <span
              className="shrink-0 rounded-full bg-muted/60 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground"
              title={`${m.plugin_count} plugin${m.plugin_count === 1 ? "" : "s"}`}
            >
              {m.plugin_count} plugin{m.plugin_count === 1 ? "" : "s"}
            </span>
          </div>
          {m.description ? (
            <p className="text-xs text-muted-foreground/90 line-clamp-3">
              {m.description}
            </p>
          ) : (
            <p className="text-[11px] italic text-muted-foreground/60">
              No description provided.
            </p>
          )}
          <p
            className="truncate font-mono text-[10px] text-muted-foreground/50"
            title={m.source}
          >
            {m.source}
          </p>
        </div>
      ))}
    </div>
  );
}

// Add marketplace dialog. Real install is deferred — submit shows a
// "coming soon" toast so the wiring is exercised end-to-end. The dialog
// only validates that the user typed *something*; deeper format
// checking is the responsibility of Claude Code itself once we hand off.
function MarketplaceAddDialog({
  onOpenChange,
}: {
  onOpenChange: (open: boolean) => void;
}) {
  const [value, setValue] = useState("");

  const handleAdd = () => {
    const src = value.trim();
    if (!src) return;
    // Placeholder behaviour for now — the real install path lands in a
    // follow-up once the underlying Claude Code command is settled.
    toast.info("Marketplace add coming soon", {
      description: src,
      duration: 4000,
    });
    setValue("");
    onOpenChange(false);
  };

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add Marketplace</DialogTitle>
          <DialogDescription>
            Paste a marketplace source. Claude Code can fetch from GitHub
            repos, remote URLs, or local paths.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-2">
          <Input
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="owner/repo · git@… · https://… · ./path"
            aria-label="Marketplace source"
            autoFocus
            onKeyDown={(e) => {
              if (e.key === "Enter") handleAdd();
            }}
          />
          <div className="space-y-0.5 text-[11px] text-muted-foreground">
            <p className="font-medium uppercase tracking-wider text-muted-foreground/70">
              Examples
            </p>
            <ul className="space-y-0.5 pl-1">
              <li className="font-mono">owner/repo</li>
              <li className="font-mono">git@github.com:owner/repo.git</li>
              <li className="font-mono">https://example.com/marketplace.json</li>
              <li className="font-mono">./path/to/marketplace</li>
            </ul>
          </div>
        </div>

        <DialogFooter>
          <DialogClose
            render={<Button variant="ghost" />}
          >
            Cancel
          </DialogClose>
          <Button
            onClick={handleAdd}
            disabled={value.trim().length === 0}
          >
            <Plus className="size-3.5" />
            Add
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

