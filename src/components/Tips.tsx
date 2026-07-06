"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Lightbulb, X } from "lucide-react";
import { TIPS } from "@/data/tips";

const STORAGE_KEY = "tips-dismissed";
const AUTO_ADVANCE_MS = 6000;

export function Tips() {
  const [hidden, setHidden] = useState(false);
  const [idx, setIdx] = useState(0);
  const [paused, setPaused] = useState(false);
  const [direction, setDirection] = useState<"next" | "prev">("next");

  useEffect(() => {
    if (typeof window !== "undefined") {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setHidden(localStorage.getItem(STORAGE_KEY) === "true");
    }
  }, []);

  const next = useCallback(() => {
    setDirection("next");
    setIdx((i) => (i + 1) % TIPS.length);
  }, []);

  const prev = useCallback(() => {
    setDirection("prev");
    setIdx((i) => (i - 1 + TIPS.length) % TIPS.length);
  }, []);

  const dismiss = useCallback(() => {
    setHidden(true);
    localStorage.setItem(STORAGE_KEY, "true");
  }, []);

  useEffect(() => {
    if (hidden || paused || TIPS.length <= 1) return;
    const timer = setInterval(next, AUTO_ADVANCE_MS);
    return () => clearInterval(timer);
  }, [hidden, paused, next]);

  const liveRegionRef = useRef<HTMLParagraphElement>(null);

  if (hidden || TIPS.length === 0) return null;

  return (
    <div
      className="rounded-lg border bg-card/60 p-3 text-left text-xs"
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
    >
      <div className="flex items-start gap-2">
        <Lightbulb className="mt-0.5 size-3.5 shrink-0 text-primary" />
        <div className="flex-1 overflow-hidden">
          <p
            key={idx}
            ref={liveRegionRef}
            aria-live="polite"
            className="leading-snug text-foreground/90 animate-in fade-in slide-in-from-bottom-1 duration-200"
            style={{
              animationName: direction === "next" ? "tip-in-next" : "tip-in-prev",
            }}
          >
            {TIPS[idx]}
          </p>
        </div>
        <button
          onClick={dismiss}
          aria-label="Dismiss tips"
          className="shrink-0 text-muted-foreground hover:text-foreground cursor-pointer transition-colors"
        >
          <X className="size-3.5" />
        </button>
      </div>

      {TIPS.length > 1 && (
        <div className="mt-2 flex items-center justify-between gap-2">
          <button
            onClick={prev}
            aria-label="Previous tip"
            className="rounded p-1 text-muted-foreground hover:text-foreground hover:bg-muted cursor-pointer transition-colors"
          >
            <ChevronLeft className="size-3.5" />
          </button>

          <div className="flex items-center gap-1">
            {TIPS.map((_, i) => (
              <button
                key={i}
                onClick={() => {
                  setDirection(i > idx ? "next" : "prev");
                  setIdx(i);
                }}
                aria-label={`Go to tip ${i + 1}`}
                aria-current={i === idx}
                className={`size-1.5 rounded-full transition-colors cursor-pointer ${
                  i === idx ? "bg-primary" : "bg-muted-foreground/30 hover:bg-muted-foreground/60"
                }`}
              />
            ))}
          </div>

          <button
            onClick={next}
            aria-label="Next tip"
            className="rounded p-1 text-muted-foreground hover:text-foreground hover:bg-muted cursor-pointer transition-colors"
          >
            <ChevronRight className="size-3.5" />
          </button>
        </div>
      )}
    </div>
  );
}