"use client";

import * as React from "react";
import { Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { LoopVideo } from "@/components/LoopVideo";

interface Props {
  hasProviders: boolean;
  onNew: () => void;
}

export function EmptyState({ hasProviders, onNew }: Props) {
  const Logo = (
    <div className="mx-auto flex justify-center select-none">
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src="/logo2.png"
        alt="Claude Config"
        className="size-24 object-contain"
      />
    </div>
  );

  if (hasProviders) {
    return (
      <div className="flex h-full items-center justify-center py-6">
        <div className="space-y-4 text-center">
          <LoopVideo
            src="/animate.mp4"
            fallbackSrc="/logo2.png"
            alt="Claude Config"
            className="mx-auto h-32 w-auto"
          />
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground">
              Or create a new one to get started.
            </p>
          </div>
          <Button size="sm" variant="default" onClick={onNew} className="cursor-pointer">
            <Plus className="size-3.5" />
            New provider
          </Button>
        </div>
      </div>
    );
  }
  return (
    <div className="flex h-full items-center justify-center py-6">
      <div className="max-w-sm space-y-4 text-center">
        {Logo}
        <div className="space-y-1">
          <p className="text-sm font-semibold">Add your first provider</p>
          <p className="text-xs text-muted-foreground leading-normal">
            Configure a base URL and auth token for the model provider you want
            to use with Claude Code. You can add as many as you like and switch
            between them with one click.
          </p>
        </div>
        <Button onClick={onNew} className="cursor-pointer">
          <Plus className="size-3.5" />
          New provider
        </Button>
      </div>
    </div>
  );
}
