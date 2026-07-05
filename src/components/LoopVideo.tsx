"use client";

import * as React from "react";
import { cn } from "@/lib/utils";

interface LoopVideoProps {
  src: string;
  fallbackSrc?: string;
  alt?: string;
  className?: string;
}

export function LoopVideo({
  src,
  fallbackSrc,
  alt = "",
  className,
}: LoopVideoProps) {
  const videoRef = React.useRef<HTMLVideoElement>(null);
  const reduceMotion = useReducedMotion();

  React.useEffect(() => {
    if (reduceMotion) return;
    const v = videoRef.current;
    if (!v) return;

    const restart = () => {
      try {
        v.currentTime = 0;
      } catch {}
      v.play().catch(() => {});
    };

    const onEnded = () => restart();
    const onTimeUpdate = () => {
      if (v.duration > 0 && v.currentTime >= v.duration - 0.05) {
        restart();
      }
    };
    const onPause = () => {
      if (!v.ended) return;
      restart();
    };

    v.addEventListener("ended", onEnded);
    v.addEventListener("timeupdate", onTimeUpdate);
    v.addEventListener("pause", onPause);
    v.play().catch(() => {});

    return () => {
      v.removeEventListener("ended", onEnded);
      v.removeEventListener("timeupdate", onTimeUpdate);
      v.removeEventListener("pause", onPause);
    };
  }, [reduceMotion]);

  if (reduceMotion && fallbackSrc) {
    return (
      // eslint-disable-next-line @next/next/no-img-element
      <img
        src={fallbackSrc}
        alt={alt}
        className={cn("select-none object-contain", className)}
      />
    );
  }

  return (
    <video
      ref={videoRef}
      src={src}
      autoPlay
      muted
      playsInline
      preload="auto"
      aria-label={alt || undefined}
      className={cn("select-none", className)}
    />
  );
}

function useReducedMotion(): boolean {
  const [reduce, setReduce] = React.useState(false);
  React.useEffect(() => {
    if (typeof window === "undefined") return;
    const mql = window.matchMedia("(prefers-reduced-motion: reduce)");
    const onChange = () => setReduce(mql.matches);
    onChange();
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, []);
  return reduce;
}
