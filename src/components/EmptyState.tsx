"use client";

import { KeyRound, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";

interface Props {
  hasProviders: boolean;
  onNew: () => void;
}

export function EmptyState({ hasProviders, onNew }: Props) {
  if (hasProviders) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="space-y-2 text-center">
          <div className="mx-auto flex size-12 items-center justify-center rounded-full bg-muted">
            <KeyRound className="size-5 text-muted-foreground" />
          </div>
          <p className="text-sm font-medium">Select a provider</p>
          <p className="text-xs text-muted-foreground">
            Or create a new one to get started.
          </p>
          <Button size="sm" variant="outline" onClick={onNew} className="mt-3">
            <Plus />
            New provider
          </Button>
        </div>
      </div>
    );
  }
  return (
    <div className="flex h-full items-center justify-center">
      <div className="max-w-sm space-y-3 text-center">
        <div className="mx-auto flex size-12 items-center justify-center rounded-full bg-muted">
          <KeyRound className="size-5 text-muted-foreground" />
        </div>
        <p className="text-base font-medium">Add your first provider</p>
        <p className="text-xs text-muted-foreground">
          Configure a base URL and auth token for the model provider you want
          to use with Claude Code. You can add as many as you like and switch
          between them with one click.
        </p>
        <Button onClick={onNew}>
          <Plus />
          New provider
        </Button>
      </div>
    </div>
  );
}