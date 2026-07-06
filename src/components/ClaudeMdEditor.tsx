"use client";

import React, { useMemo } from "react";
import {
  FilePlus,
  FileText,
  HelpCircle,
  Loader2,
  Save,
  RotateCcw,
  Sparkles,
  ArrowLeft,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useClaudeMd, useClaudeMdExists } from "@/hooks/useClaudeMd";
import type {
  GlobalTabProps,
  SidebarTabButtonProps,
} from "@/data/globalTabs";
import { cn } from "@/lib/utils";

// ponytail: preview-pane markdown rendering removed (~80 LOC + the
// security-sensitive `dangerouslySetInnerHTML` path). Users edit markdown
// they'll recognise on the file system; the editor reports save state via
// a chip. Reintroduce a renderer only when one is needed; if so, use a real
// markdown library (react-markdown + rehype-sanitize) instead of a regex
// pipeline we hand-rolled. Adding a tab = one entry in data/globalTabs.ts
// and one self-contained component (editor + sidebar button live together).

/**
 * Sidebar entry for the CLAUDE.md tab. Owns the "+ Add CLAUDE.md" ↔ "CLAUDE.md"
 * label distinction because that's tab-domain: settings.json or hooks probably
 * don't have an "absent" state to communicate. Renders the file label
 * optimistically until the existence probe lands, then settles to "+ Add"
 * if the file truly doesn't exist.
 */
export function ClaudeMdSidebarButton({
  active,
  onSelect,
}: SidebarTabButtonProps) {
  const exists = useClaudeMdExists();
  const loaded = exists !== null;
  const label = loaded && !exists ? "+ Add CLAUDE.md" : "CLAUDE.md";
  const Icon = loaded && !exists ? FilePlus : FileText;
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
      <Icon
        className={cn(
          "size-3.5 shrink-0",
          active
            ? "text-primary"
            : "text-muted-foreground group-hover:text-foreground",
        )}
      />
      <span className="flex-1 truncate">{label}</span>
    </button>
  );
}

export function ClaudeMdEditor({ onClose }: GlobalTabProps) {
  const {
    editorContent,
    setEditorContent,
    loading,
    saving,
    hasChanges,
    fileExists,
    save,
    reset,
  } = useClaudeMd();

  const handleSave = async () => {
    if (!hasChanges || saving) return;
    await save();
  };

  const handleTextareaKeyDown = (
    e: React.KeyboardEvent<HTMLTextAreaElement>,
  ) => {
    if (e.key === "Tab") {
      e.preventDefault();
      const textarea = e.currentTarget;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;
      const val = textarea.value;
      const newVal = val.substring(0, start) + "  " + val.substring(end);
      setEditorContent(newVal);

      setTimeout(() => {
        textarea.selectionStart = textarea.selectionEnd = start + 2;
      }, 0);
    }
  };

  const lineCount = useMemo(
    () => editorContent.split("\n").length,
    [editorContent],
  );

  return (
    <Card className="border-border/60 flex flex-col h-full overflow-hidden min-h-125">
      <CardHeader className="pb-3 border-b shrink-0 bg-muted/5">
        <div className="flex items-center gap-3">
          <button
            onClick={onClose}
            className="text-muted-foreground hover:text-foreground rounded p-1 cursor-pointer transition-colors"
            title="Back to providers"
          >
            <ArrowLeft className="size-4" />
          </button>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <CardTitle className="text-base font-semibold flex items-center gap-1.5">
                <FileText className="size-4 text-primary shrink-0" />
                <span>CLAUDE.md</span>
              </CardTitle>

              <span
                className={cn(
                  "text-[9px] font-medium px-2 py-0.5 rounded-sm border select-none transition-all duration-150",
                  !fileExists &&
                    "bg-amber-500/10 text-amber-500 border-amber-500/20",
                  fileExists &&
                    !hasChanges &&
                    "bg-emerald-500/10 text-emerald-500 border-emerald-500/20",
                  fileExists &&
                    hasChanges &&
                    "bg-amber-500/10 text-amber-500 border-amber-500/20 animate-pulse",
                )}
              >
                {!fileExists
                  ? "absent"
                  : hasChanges
                    ? "unsaved changes"
                    : "saved"}
              </span>

              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger className="text-muted-foreground hover:text-foreground transition-colors cursor-help">
                    <HelpCircle className="size-3.5" />
                  </TooltipTrigger>
                  <TooltipContent side="right">
                    Give global instructions to Claude Code from here. These
                    rules apply across all repositories you run Claude Code
                    in.
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            </div>
            <p className="mt-0.5 text-[10px] text-muted-foreground truncate">
              Global agentic rules &amp; coding standards configuration
            </p>
          </div>
        </div>
      </CardHeader>

      <CardContent className="p-0 flex-1 min-h-0 flex flex-col relative bg-card/10">
        {loading ? (
          <div className="absolute inset-0 flex items-center justify-center bg-card/60 z-10">
            <div className="flex flex-col items-center gap-2">
              <Loader2 className="size-6 animate-spin text-primary" />
              <p className="text-xs text-muted-foreground">Loading file...</p>
            </div>
          </div>
        ) : null}

        <div className="flex-1 min-h-0 overflow-y-auto divide-x divide-border/40 flex">
          <div className="hidden sm:flex flex-col select-none text-right font-mono text-[10px] text-muted-foreground/40 bg-muted/5 px-2.5 py-3.5 space-y-1 min-w-12">
            {Array.from({ length: Math.max(1, lineCount) }).map((_, i) => (
              <div key={i} className="h-5 leading-5">
                {i + 1}
              </div>
            ))}
          </div>

          <div className="flex-1 min-w-0">
            <textarea
              value={editorContent}
              onChange={(e) => setEditorContent(e.target.value)}
              onKeyDown={handleTextareaKeyDown}
              placeholder={`# Global Instructions for Claude

Describe coding standards, preferred tools, testing requirements, or general behavior guidelines here.
Example:
- Always use TypeScript with strict mode enabled.
- Write unit tests using Vitest or Jest.
- Follow ES6+ style conventions.`}
              className="w-full min-h-95 p-3.5 font-mono text-xs text-foreground bg-transparent border-0 outline-none resize-none focus:ring-0 leading-5 field-sizing-content"
              spellCheck={false}
            />
          </div>
        </div>
      </CardContent>

      <div className="px-4 py-3 border-t shrink-0 flex items-center justify-between bg-muted/5 text-xs">
        <div className="text-muted-foreground flex items-center gap-1">
          <Sparkles className="size-3 text-primary/80" />
          <span>
            Instructions will be stored in your Claude config directory.
          </span>
        </div>
        <div className="flex items-center gap-2">
          {hasChanges && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={reset}
              disabled={saving}
              className="cursor-pointer"
            >
              <RotateCcw className="size-3" />
              Revert
            </Button>
          )}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onClose}
            disabled={saving}
            className="cursor-pointer"
          >
            Cancel
          </Button>
          <Button
            type="button"
            size="sm"
            onClick={handleSave}
            disabled={!hasChanges || saving}
            className="cursor-pointer"
          >
            {saving ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <Save className="size-3" />
            )}
            {saving ? "Saving..." : "Save Instructions"}
          </Button>
        </div>
      </div>
    </Card>
  );
}
