# Frontend Shell Responsiveness Design

Date: 2026-07-05
Project: Claude Config
Status: Approved for planning

## Goal

Improve the Tauri frontend so the main shell is more scalable and maintainable,
while fixing the current responsive failure where the provider sidebar and main
content squeeze each other as the window narrows.

The redesigned shell must:

- keep providers on the left side of the app
- collapse the sidebar on narrower widths instead of stacking or moving it to
  the bottom
- preserve a readable main content area without compressed form controls
- break shell responsibilities into more isolated components to prevent
  `src/app/page.tsx` from growing further

## Current State

The current frontend already has a reasonable leaf-component split:

- `ProviderList`
- `ProviderForm`
- `ActiveBanner`
- `CustomConfigBanner`
- `KeyringWarning`
- `DeleteDialog`
- `SettingsMenu`
- `TitleBar`

The main issue is that `src/app/page.tsx` still owns too many responsibilities
at once:

- initial data loading
- provider CRUD and load actions
- modal state
- form/editing state
- shell layout composition
- responsive behavior

The responsive bug comes from the shell using a fixed side-by-side flex layout
while `ProviderList` is pinned to a fixed width. As the window narrows, the
main content has no strong protection against compression, so the UI becomes
crowded instead of adapting.

## Chosen Approach

Use a collapsible left rail.

Behavior:

- expanded on wider windows
- automatically collapsed below a width threshold
- manually expandable and collapsible with a Lucide toggle control
- always left-aligned; never moved below the content

This approach preserves the desktop information architecture, matches the user
requirement, and requires less behavioral change than converting the provider
list into a drawer.

## Architecture Changes

`src/app/page.tsx` remains the top-level state owner for provider data and
actions because the current project architecture explicitly documents that page
state lives there.

However, the shell rendering and responsive sidebar logic move out into focused
components so the page file stops accumulating layout logic.

### New responsibilities

`src/app/page.tsx`
- owns provider data and async actions
- owns mode, delete target, loading, and saving state
- passes a shell view model into layout components

`src/components/AppShell.tsx`
- owns the overall window body composition under the title bar
- arranges sidebar and content panes
- receives already-prepared callbacks and state from `page.tsx`

`src/components/ProviderSidebar.tsx`
- wraps provider navigation behavior
- renders collapse toggle and sidebar header
- chooses expanded or collapsed presentation
- contains sidebar width and overflow rules

`src/components/ProviderList.tsx`
- becomes a provider list presentation component
- supports `expanded` and `collapsed` display modes
- stops owning outer shell width assumptions

`src/components/ProviderCard.tsx`
- supports a compact presentation for collapsed sidebar mode
- preserves active, selected, loading, and delete affordances

`src/components/ShellContent.tsx`
- wraps the right-side content column
- owns readable width constraints and scroll container structure

`src/components/useSidebarCollapse.ts`
- derives auto-collapse from viewport/window width
- merges automatic behavior with manual user toggling
- exposes a small state contract to the sidebar shell

## Sidebar State Model

Use a small explicit state model instead of scattering conditional class names
throughout the page.

Suggested model:

- `expanded`
- `collapsed`

Supporting metadata:

- `isAutoCollapsed: boolean`
- `canExpand: boolean`

Rules:

- on large enough widths, default to expanded
- below a layout threshold, auto-collapse the sidebar
- if the user manually collapses on a wide screen, preserve that preference
  until the window crosses the auto-collapse threshold
- if the screen is too narrow for expanded mode, the sidebar cannot force the
  content pane to shrink below its minimum readable width

The implementation should prefer `window.innerWidth` or a `matchMedia`-driven
hook rather than relying only on CSS visibility classes, because collapse state
also affects interaction and component rendering.

## Layout Rules

The shell must be resilient under shrinking width.

### Sidebar rules

- expanded width: fixed, approximately `18rem`
- collapsed width: fixed, approximately `4rem`
- sidebar must `shrink-0`
- sidebar gets its own vertical scroll region for the provider list
- collapse toggle remains visible in both states

### Main content rules

- content pane must use `min-w-0` so internal overflow is handled correctly
- content area gets a readable inner max width instead of inheriting pressure
  from the sidebar
- form controls should reflow vertically before compressing horizontally
- no horizontal page scroll in the normal shell state

### Window-level behavior

- title bar remains fixed at the top
- sidebar and main content each scroll independently as needed
- the shell must continue to work in a narrow Tauri window without text or
  controls overlapping

## Responsive Behavior

The responsive strategy is:

1. Wide width: expanded provider sidebar
2. Medium width: optional manual collapse, but content remains comfortable
3. Narrow width: auto-collapsed sidebar takes priority over squeezing the form

This is intentionally not a mobile-web pattern. The app is a desktop Tauri app,
so the responsive behavior should optimize for resizable windows rather than
phone layouts.

## Componentization Principles

The refactor should isolate responsibilities by behavior rather than by visual
fragments alone.

### Good boundaries for this app

- page-level container for data fetching and mutations
- shell/layout components for structural composition
- navigation/sidebar components for provider browsing behavior
- content wrapper for banners, empty state, and form region
- small hook for sidebar responsive state

### Boundaries to avoid

- moving provider business logic into purely visual components
- duplicating provider action wiring in multiple places
- creating many one-line wrapper components with no responsibility
- splitting state across unrelated files without a clear ownership model

## Scalability Assessment

### What is already maintainable

- typed API boundary in `src/lib/api.ts`
- reusable UI components under `src/components/`
- clean separation between frontend types and Rust backend contracts

### What currently limits scalability

- `src/app/page.tsx` mixes domain actions with shell layout details
- responsive behavior is not modeled as a first-class concern
- fixed-width sidebar assumptions leak from the navigation component into the
  overall layout

### What this design improves

- layout concerns become isolated and easier to evolve
- sidebar behavior becomes explicit and testable
- provider UI can grow without forcing `page.tsx` to absorb more rendering logic
- future shell enhancements, such as filters or grouped providers, have a
  clearer place to live

## Error Handling and Edge Cases

- empty provider list must still work in expanded and collapsed modes
- long provider names must truncate cleanly without stretching the sidebar
- loading and deleting states must remain visible in collapsed mode
- keyboard focus must still reach the collapse toggle and provider actions
- if window width changes while editing a provider, the form layout must remain
  usable without clipped controls

## Testing Strategy

Because this repository has no frontend unit-test runner, verification should be
done through lint, typecheck, and manual resizing in the Tauri shell.

Required verification:

- `pnpm lint`
- `pnpm exec tsc --noEmit`
- manual resize testing in `pnpm tauri dev`

Manual checks:

- sidebar expands on wide windows
- sidebar auto-collapses on narrower windows
- manual toggle works when width allows it
- content does not get crushed when shrinking the app window
- provider creation and editing remain usable in both sidebar states

## Out of Scope

- changing provider CRUD flows
- changing backend commands or data contracts
- redesigning the visual identity of the app
- moving app state ownership out of `src/app/page.tsx`
- introducing a full global state library

## Implementation Outline

1. Extract shell composition from `src/app/page.tsx` into `AppShell` and
   `ShellContent`
2. Introduce `ProviderSidebar` and a sidebar collapse hook
3. Refactor `ProviderList` and `ProviderCard` to support collapsed presentation
4. Replace fixed layout assumptions with explicit width and overflow rules
5. Verify responsive behavior by resizing the Tauri window and checking that
   the main content remains readable

## Notes

No git commit is included at the design stage because committing was not
requested in this session.
