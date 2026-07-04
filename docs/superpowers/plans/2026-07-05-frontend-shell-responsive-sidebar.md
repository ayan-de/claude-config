# Frontend Shell Responsive Sidebar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the frontend shell into smaller layout components and add a collapsible left provider sidebar that prevents the main content from being squeezed as the Tauri window narrows.

**Architecture:** Keep `src/app/page.tsx` as the page-level owner of provider data and async actions, but move shell composition and responsive sidebar behavior into focused components. Model sidebar collapse explicitly with a small hook so layout behavior is isolated, predictable, and easy to evolve.

**Tech Stack:** Next.js 16 App Router client components, React 19, TypeScript, Tailwind v4, lucide-react, Tauri desktop shell

---

## File Structure

### Create

- `src/components/AppShell.tsx` - top-level shell composition for sidebar + content region
- `src/components/ProviderSidebar.tsx` - left navigation shell with collapse toggle and width behavior
- `src/components/ShellContent.tsx` - right-side content container with readable width and scroll rules
- `src/components/useSidebarCollapse.ts` - window-width driven collapse state hook with manual override support
- `docs/superpowers/plans/2026-07-05-frontend-shell-responsive-sidebar.md` - this implementation plan

### Modify

- `src/app/page.tsx` - replace inline shell layout with extracted shell components and pass layout props downward
- `src/components/ProviderList.tsx` - support `expanded` and `collapsed` modes instead of fixed-width ownership
- `src/components/ProviderCard.tsx` - add compact rendering path for collapsed sidebar mode
- `src/components/TitleBar.tsx` - make action area more resilient when width gets tighter

### Verify

- `pnpm lint`
- `pnpm exec tsc --noEmit`
- `pnpm tauri dev`

## Implementation Notes

- This repository does not have a frontend unit-test runner, so task verification uses typecheck, lint, and manual Tauri resize checks.
- Preserve the existing architecture rule from the project docs: `src/app/page.tsx` remains the top-level app state owner.
- Do not change provider CRUD behavior or typed API contracts.

### Task 1: Extract Shell Layout Components

**Files:**
- Create: `src/components/AppShell.tsx`
- Create: `src/components/ShellContent.tsx`
- Modify: `src/app/page.tsx`
- Verify: `pnpm exec tsc --noEmit`

- [ ] **Step 1: Add the new shell component interface before wiring it into the page**

Create `src/components/AppShell.tsx` with this shape:

```tsx
import type { ReactNode } from "react";

interface AppShellProps {
  sidebar: ReactNode;
  content: ReactNode;
}

export function AppShell({ sidebar, content }: AppShellProps) {
  return (
    <div className="flex min-h-0 flex-1 overflow-hidden">
      {sidebar}
      {content}
    </div>
  );
}
```

Create `src/components/ShellContent.tsx` with this shape:

```tsx
import type { ReactNode } from "react";

interface ShellContentProps {
  children: ReactNode;
}

export function ShellContent({ children }: ShellContentProps) {
  return (
    <main className="min-w-0 flex-1 overflow-y-auto p-4 sm:p-6">
      <div className="mx-auto w-full max-w-2xl space-y-4">{children}</div>
    </main>
  );
}
```

- [ ] **Step 2: Run typecheck before page wiring to confirm the new files compile cleanly**

Run: `pnpm exec tsc --noEmit`

Expected: PASS with no TypeScript errors from `AppShell` or `ShellContent`

- [ ] **Step 3: Replace the inline flex shell in `src/app/page.tsx` with the new components**

Update the layout section in `src/app/page.tsx` from the current inline structure:

```tsx
<div className="flex min-h-0 flex-1">
  <ProviderList ... />
  <main className="flex-1 overflow-y-auto p-6">
    <div className="mx-auto max-w-2xl space-y-4">...</div>
  </main>
</div>
```

to this structure:

```tsx
<AppShell
  sidebar={
    <ProviderList
      providers={providers}
      activeProviderId={active?.id ?? null}
      selectedId={editingProvider?.id ?? null}
      loadingId={loadingId}
      onSelect={handleSelect}
      onLoad={handleLoad}
      onDelete={(id) => {
        const p = providers.find((x) => x.id === id);
        if (p) setDeleteTarget(p);
      }}
      onNew={handleNew}
    />
  }
  content={
    <ShellContent>
      <KeyringWarning status={keyring} />
      {active && !showForm && <ActiveBanner provider={active} />}
      {customEnvKeys && !showForm && (
        <CustomConfigBanner envKeys={customEnvKeys} onSaveAs={handleSaveCurrentAs} />
      )}
      {showForm ? (
        <ProviderForm
          editing={editingProvider}
          onCancel={handleCancel}
          onSave={handleSave}
          isSaving={saving}
        />
      ) : (
        <EmptyState hasProviders={providers.length > 0} onNew={handleNew} />
      )}
    </ShellContent>
  }
/>
```

- [ ] **Step 4: Re-run typecheck after replacing the page layout**

Run: `pnpm exec tsc --noEmit`

Expected: PASS and no missing import or prop errors in `src/app/page.tsx`

- [ ] **Step 5: Commit the extraction checkpoint**

```bash
git add src/app/page.tsx src/components/AppShell.tsx src/components/ShellContent.tsx
git commit -m "refactor: extract app shell layout"
```

### Task 2: Add Responsive Sidebar State Hook

**Files:**
- Create: `src/components/useSidebarCollapse.ts`
- Verify: `pnpm exec tsc --noEmit`

- [ ] **Step 1: Write the hook with explicit state and manual override behavior**

Create `src/components/useSidebarCollapse.ts` with this implementation:

```tsx
"use client";

import { useEffect, useMemo, useState } from "react";

const AUTO_COLLAPSE_MAX_WIDTH = 1100;

export interface SidebarCollapseState {
  collapsed: boolean;
  isAutoCollapsed: boolean;
  canExpand: boolean;
  toggle: () => void;
}

export function useSidebarCollapse(): SidebarCollapseState {
  const [windowWidth, setWindowWidth] = useState<number | null>(null);
  const [manualCollapsed, setManualCollapsed] = useState(false);

  useEffect(() => {
    const update = () => setWindowWidth(window.innerWidth);
    update();
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, []);

  const isAutoCollapsed =
    windowWidth !== null && windowWidth <= AUTO_COLLAPSE_MAX_WIDTH;

  const collapsed = isAutoCollapsed || manualCollapsed;
  const canExpand = !isAutoCollapsed;

  return useMemo(
    () => ({
      collapsed,
      isAutoCollapsed,
      canExpand,
      toggle: () => {
        if (isAutoCollapsed) return;
        setManualCollapsed((value) => !value);
      },
    }),
    [collapsed, isAutoCollapsed, canExpand],
  );
}
```

- [ ] **Step 2: Run typecheck to validate hook exports before consumer wiring**

Run: `pnpm exec tsc --noEmit`

Expected: PASS and exported `SidebarCollapseState` resolves correctly

- [ ] **Step 3: Commit the hook checkpoint**

```bash
git add src/components/useSidebarCollapse.ts
git commit -m "feat: add sidebar collapse hook"
```

### Task 3: Build the Sidebar Shell and Collapse Toggle

**Files:**
- Create: `src/components/ProviderSidebar.tsx`
- Modify: `src/components/ProviderList.tsx`
- Modify: `src/app/page.tsx`
- Verify: `pnpm exec tsc --noEmit`

- [ ] **Step 1: Create `ProviderSidebar` so width behavior stops living in `ProviderList`**

Create `src/components/ProviderSidebar.tsx` with this structure:

```tsx
"use client";

import { PanelLeftClose, PanelLeftOpen, Plus } from "lucide-react";

import { Button } from "@/components/ui/button";
import { ProviderList } from "@/components/ProviderList";
import { cn } from "@/lib/utils";
import type { Provider } from "@/lib/types";
import type { SidebarCollapseState } from "@/components/useSidebarCollapse";

interface ProviderSidebarProps {
  providers: Provider[];
  activeProviderId: string | null;
  selectedId: string | null;
  loadingId: string | null;
  collapse: SidebarCollapseState;
  onSelect: (id: string) => void;
  onLoad: (id: string) => void;
  onDelete: (id: string) => void;
  onNew: () => void;
}

export function ProviderSidebar({
  providers,
  activeProviderId,
  selectedId,
  loadingId,
  collapse,
  onSelect,
  onLoad,
  onDelete,
  onNew,
}: ProviderSidebarProps) {
  return (
    <aside
      className={cn(
        "flex h-full shrink-0 flex-col border-r bg-card/30 transition-[width] duration-200",
        collapse.collapsed ? "w-16" : "w-72",
      )}
    >
      <div className="flex items-center justify-between gap-2 px-3 py-3">
        {collapse.collapsed ? (
          <Button size="icon" variant="ghost" onClick={onNew} aria-label="New provider">
            <Plus className="size-4" />
          </Button>
        ) : (
          <>
            <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              Providers ({providers.length})
            </h2>
            <Button size="sm" variant="ghost" onClick={onNew} className="h-7 px-2">
              <Plus className="size-3.5" />
              New
            </Button>
          </>
        )}

        <Button
          size="icon"
          variant="ghost"
          onClick={collapse.toggle}
          disabled={!collapse.canExpand && collapse.isAutoCollapsed}
          aria-label={collapse.collapsed ? "Expand sidebar" : "Collapse sidebar"}
          title={collapse.isAutoCollapsed ? "Sidebar auto-collapsed for window size" : undefined}
          className="h-8 w-8"
        >
          {collapse.collapsed ? (
            <PanelLeftOpen className="size-4" />
          ) : (
            <PanelLeftClose className="size-4" />
          )}
        </Button>
      </div>

      <ProviderList
        providers={providers}
        activeProviderId={activeProviderId}
        selectedId={selectedId}
        loadingId={loadingId}
        collapsed={collapse.collapsed}
        onSelect={onSelect}
        onLoad={onLoad}
        onDelete={onDelete}
        onNew={onNew}
      />
    </aside>
  );
}
```

- [ ] **Step 2: Refactor `ProviderList` to become pure list presentation with a `collapsed` prop**

Update the `Props` interface and wrapper in `src/components/ProviderList.tsx` to this shape:

```tsx
interface Props {
  providers: Provider[];
  activeProviderId: string | null;
  selectedId: string | null;
  loadingId: string | null;
  collapsed: boolean;
  onSelect: (id: string) => void;
  onLoad: (id: string) => void;
  onDelete: (id: string) => void;
  onNew: () => void;
}

export function ProviderList({
  providers,
  activeProviderId,
  selectedId,
  loadingId,
  collapsed,
  onSelect,
  onLoad,
  onDelete,
  onNew,
}: Props) {
  return (
    <div className="flex-1 overflow-y-auto px-2 pb-3">
      {providers.length === 0 ? (
        collapsed ? (
          <div className="flex items-center justify-center py-4">
            <Button size="icon" variant="outline" onClick={onNew} aria-label="Add your first provider">
              <Plus className="size-4" />
            </Button>
          </div>
        ) : (
          <div className="rounded-lg border border-dashed p-6 text-center">
            <p className="text-xs text-muted-foreground">No providers yet</p>
            <Button size="sm" variant="outline" onClick={onNew} className="mt-3">
              <Plus className="size-3.5" />
              Add your first
            </Button>
          </div>
        )
      ) : (
        <div className="space-y-2">
          {providers.map((p) => (
            <ProviderCard
              key={p.id}
              provider={p}
              collapsed={collapsed}
              isActive={p.id === activeProviderId}
              isSelected={p.id === selectedId}
              isLoading={p.id === loadingId}
              onSelect={() => onSelect(p.id)}
              onLoad={() => onLoad(p.id)}
              onDelete={() => onDelete(p.id)}
            />
          ))}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Wire the new sidebar into `src/app/page.tsx` using the collapse hook**

Add imports:

```tsx
import { AppShell } from "@/components/AppShell";
import { ProviderSidebar } from "@/components/ProviderSidebar";
import { ShellContent } from "@/components/ShellContent";
import { useSidebarCollapse } from "@/components/useSidebarCollapse";
```

Add hook usage near the other state setup:

```tsx
const sidebarCollapse = useSidebarCollapse();
```

Replace the sidebar prop from Task 1 with:

```tsx
<ProviderSidebar
  providers={providers}
  activeProviderId={active?.id ?? null}
  selectedId={editingProvider?.id ?? null}
  loadingId={loadingId}
  collapse={sidebarCollapse}
  onSelect={handleSelect}
  onLoad={handleLoad}
  onDelete={(id) => {
    const p = providers.find((x) => x.id === id);
    if (p) setDeleteTarget(p);
  }}
  onNew={handleNew}
/>
```

- [ ] **Step 4: Run typecheck after sidebar wiring**

Run: `pnpm exec tsc --noEmit`

Expected: PASS and no prop drift between `page.tsx`, `ProviderSidebar`, and `ProviderList`

- [ ] **Step 5: Commit the sidebar shell checkpoint**

```bash
git add src/app/page.tsx src/components/ProviderSidebar.tsx src/components/ProviderList.tsx
git commit -m "feat: add collapsible provider sidebar"
```

### Task 4: Add Collapsed Provider Card Presentation

**Files:**
- Modify: `src/components/ProviderCard.tsx`
- Verify: `pnpm exec tsc --noEmit`

- [ ] **Step 1: Add a `collapsed` prop so cards can render compact navigation rows**

Update the props in `src/components/ProviderCard.tsx`:

```tsx
interface Props {
  provider: Provider;
  collapsed: boolean;
  isActive: boolean;
  isSelected: boolean;
  isLoading: boolean;
  onSelect: () => void;
  onLoad: () => void;
  onDelete: () => void;
}
```

Add this helper near the `host` computation:

```tsx
const initial = provider.name.trim().charAt(0).toUpperCase() || "?";
```

- [ ] **Step 2: Replace the single layout with expanded and collapsed rendering paths**

Use this render structure inside `ProviderCard`:

```tsx
if (collapsed) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        "group flex w-full items-center justify-center rounded-lg border bg-card p-2 transition-colors",
        isSelected
          ? "border-foreground/30 bg-card/80 ring-1 ring-foreground/10"
          : "border-border hover:border-foreground/20",
      )}
      aria-label={provider.name}
      title={`${provider.name}${isActive ? " (active)" : ""}`}
    >
      <div className="relative flex size-9 items-center justify-center rounded-md bg-muted text-xs font-semibold">
        <span
          className={cn(
            "absolute right-1 top-1 size-1.5 rounded-full",
            isActive ? "bg-emerald-400" : "bg-muted-foreground/30",
          )}
        />
        {isLoading ? <Loader2 className="size-3 animate-spin" /> : initial}
      </div>
    </button>
  );
}

return (
  <div
    className={cn(
      "group relative cursor-pointer rounded-lg border bg-card p-3 transition-colors",
      isSelected
        ? "border-foreground/30 bg-card/80 ring-1 ring-foreground/10"
        : "border-border hover:border-foreground/20",
    )}
    onClick={onSelect}
  >
    <div className="flex items-start justify-between gap-2">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span
            className={cn(
              "size-1.5 shrink-0 rounded-full",
              isActive ? "bg-emerald-400" : "bg-muted-foreground/30",
            )}
            title={isActive ? "Active" : "Inactive"}
          />
          <span className="truncate text-sm font-medium">{provider.name}</span>
        </div>
        <p className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">{host}</p>
      </div>
    </div>

    <div
      className={cn(
        "mt-3 flex items-center gap-1.5 transition-opacity",
        isSelected ? "opacity-100" : "opacity-0 group-hover:opacity-100",
      )}
    >
      <Button
        size="sm"
        variant="default"
        onClick={(e) => {
          e.stopPropagation();
          onLoad();
        }}
        disabled={isLoading}
        className="h-7 flex-1 px-2 text-xs"
      >
        {isLoading ? <Loader2 className="size-3 animate-spin" /> : <Play className="size-3" />}
        Load
      </Button>
      <Button
        size="sm"
        variant="outline"
        onClick={(e) => {
          e.stopPropagation();
          onSelect();
        }}
        className="h-7 px-2"
        aria-label="Edit"
      >
        <Pencil className="size-3" />
      </Button>
      <Button
        size="sm"
        variant="outline"
        onClick={(e) => {
          e.stopPropagation();
          onDelete();
        }}
        className="h-7 px-2 hover:bg-destructive/10 hover:text-destructive"
        aria-label="Delete"
      >
        <Trash2 className="size-3" />
      </Button>
    </div>
  </div>
);
```

- [ ] **Step 3: Run typecheck to verify the new prop path is consistent**

Run: `pnpm exec tsc --noEmit`

Expected: PASS and no missing `collapsed` prop errors

- [ ] **Step 4: Commit the compact card checkpoint**

```bash
git add src/components/ProviderCard.tsx src/components/ProviderList.tsx
git commit -m "feat: add collapsed provider card mode"
```

### Task 5: Harden the Title Bar and Main Content Against Narrow Widths

**Files:**
- Modify: `src/components/TitleBar.tsx`
- Modify: `src/app/page.tsx`
- Verify: `pnpm exec tsc --noEmit`

- [ ] **Step 1: Let title bar content shrink and wrap instead of pushing into the window controls**

Adjust the outer structure in `src/components/TitleBar.tsx`:

```tsx
return (
  <div className="flex h-11 w-full items-center justify-between gap-2 border-b bg-card/30 pl-3 pr-1 select-none tauri-drag">
    <div className="min-w-0 flex items-center gap-3">{...}</div>
    <div className="tauri-no-drag flex shrink-0 items-center gap-0.5">{...}</div>
  </div>
);
```

Also constrain the left title content in `src/app/page.tsx`:

```tsx
left={
  <div className="flex min-w-0 items-center gap-3">
    <div className="flex size-8 shrink-0 items-center justify-center rounded-sm bg-[#f4f3ee]">
      <Image src="/logo.png" alt="Claude Config" width={28} height={28} />
    </div>
    <div className="min-w-0">
      <h1 className="truncate text-sm font-semibold leading-none">Claude Config</h1>
      <p className="mt-0.5 truncate text-[10px] text-muted-foreground">
        Manage Claude Code providers
      </p>
    </div>
  </div>
}
```

- [ ] **Step 2: Make the title-bar actions compress more gracefully**

Change the `actions` wrapper in `src/app/page.tsx` to:

```tsx
actions={
  <div className="flex items-center gap-2 pr-2">
    <Button
      size="sm"
      onClick={handleNew}
      disabled={!keyringAvailable}
      className="max-sm:px-2"
    >
      <Plus className="size-3.5" />
      <span className="max-sm:sr-only">New provider</span>
      <span className="sm:not-sr-only hidden">New provider</span>
    </Button>
    <SettingsMenu
      appDataDir={appDataDir}
      claudeDir={claudeDir}
      onRevealAppDir={handleRevealAppDir}
      onRevealClaudeDir={handleRevealClaudeDir}
      onExport={handleExport}
      onImport={handleImport}
    />
  </div>
}
```

- [ ] **Step 3: Run typecheck after the width-hardening pass**

Run: `pnpm exec tsc --noEmit`

Expected: PASS with no JSX or className syntax issues

- [ ] **Step 4: Commit the narrow-width layout checkpoint**

```bash
git add src/app/page.tsx src/components/TitleBar.tsx
git commit -m "refactor: improve narrow window shell behavior"
```

### Task 6: Verify Responsive Behavior End-to-End

**Files:**
- Modify: none expected unless issues are found during verification
- Verify: `pnpm lint`
- Verify: `pnpm exec tsc --noEmit`
- Verify: `pnpm tauri dev`

- [ ] **Step 1: Run ESLint for the touched frontend files**

Run: `pnpm lint`

Expected: PASS with no new lint errors

- [ ] **Step 2: Run full TypeScript typecheck**

Run: `pnpm exec tsc --noEmit`

Expected: PASS with no type regressions

- [ ] **Step 3: Launch the Tauri app and manually resize the window**

Run: `pnpm tauri dev`

Manual verification checklist:

```text
1. Start with a wide window: sidebar is expanded and provider cards show full content.
2. Narrow the window gradually: sidebar auto-collapses before the form becomes cramped.
3. On narrow width, confirm the right content area remains readable and controls do not overlap.
4. Re-expand the window: manual toggle works again and expanded sidebar returns cleanly.
5. Select, load, create, and edit a provider in both sidebar states.
6. Confirm no horizontal page scroll appears during normal shell use.
```

Expected: PASS on all six manual checks

- [ ] **Step 4: If verification reveals regressions, fix them before finalizing**

Likely adjustment targets:

```text
- tweak auto-collapse threshold in useSidebarCollapse.ts
- reduce sidebar collapsed width from w-16 to w-14 only if controls still fit
- tighten ShellContent padding from p-4 sm:p-6 to p-4 sm:p-5 if the form feels cramped
- add truncate/min-w-0 to any title or form row that still overflows
```

- [ ] **Step 5: Commit the verified responsive shell**

```bash
git add src/app/page.tsx src/components/TitleBar.tsx src/components/AppShell.tsx src/components/ShellContent.tsx src/components/ProviderSidebar.tsx src/components/ProviderList.tsx src/components/ProviderCard.tsx src/components/useSidebarCollapse.ts
git commit -m "feat: make provider shell responsive"
```

## Self-Review

- Spec coverage check: the plan includes shell extraction, collapsible left sidebar behavior, explicit sidebar state, main-content anti-squeeze rules, title bar resilience, and end-to-end resize verification.
- Placeholder scan: no `TODO`, `TBD`, or deferred implementation markers remain.
- Type consistency check: `collapsed`, `SidebarCollapseState`, `ProviderSidebar`, `AppShell`, and `ShellContent` are named consistently across all tasks.
