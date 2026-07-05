"use client";

import { sanitizeSvg } from "@/lib/utils-app";
import { cn } from "@/lib/utils";

interface Props {
  /** Inline SVG markup. Falsy → render a neutral placeholder dot. */
  svg?: string | null;
  /** Pixel size of the bounding box (square). Logo scales within. */
  size?: number;
  className?: string;
}

/**
 * Render an inline SVG provider logo. SVGs are theme-aware via
 * `currentColor` — the wrapper sets `color: var(--logo-color)` so the
 * SVG inherits the brand copper in light mode and the warm off-white
 * in dark mode without per-SVG theming logic.
 *
 * Sanitization runs at render time as defense-in-depth even for built-in
 * presets, and is mandatory for user-uploaded SVGs.
 */
export function ProviderLogo({ svg, size = 24, className }: Props) {
  const clean = svg ? sanitizeSvg(svg) : "";

  if (!clean) {
    return (
      <span
        aria-hidden="true"
        className={cn(
          "inline-block shrink-0 rounded-full bg-muted-foreground/25",
          className,
        )}
        style={{ width: size, height: size }}
      />
    );
  }

  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center justify-center [&>svg]:h-full [&>svg]:w-full [&>svg]:max-h-full [&>svg]:max-w-full",
        className,
      )}
      style={{
        width: size,
        height: size,
        // Inline fallback to copper so the logo renders even if the build
        // pipeline strips the theme variable for any reason.
        color: "var(--logo-color, #c15f3c)",
      }}
      // sanitizeSvg strips scripts and event handlers; preset SVGs are
      // authored by us and uploaded SVGs go through DOMPurify before save.
      dangerouslySetInnerHTML={{ __html: clean }}
    />
  );
}