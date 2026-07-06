/* eslint-disable react-hooks/set-state-in-effect --
 * The data-fetching useEffect here triggers an initial CLAUDE.md load
 * on mount.
 */
"use client";

import { useState, useCallback, useEffect } from "react";
import { toast } from "sonner";
import { claudeMdExists, readClaudeMd, writeClaudeMd } from "@/lib/api";

export function useClaudeMd() {
  const [content, setContent] = useState<string | null>(null);
  const [editorContent, setEditorContent] = useState<string>("");
  const [loading, setLoading] = useState<boolean>(false);
  const [saving, setSaving] = useState<boolean>(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const data = await readClaudeMd();
      setContent(data);
      setEditorContent(data ?? "");
    } catch (e) {
      toast.error(`Failed to load CLAUDE.md: ${(e as Error).message}`);
    } finally {
      setLoading(false);
    }
  }, []);

  const save = useCallback(async () => {
    setSaving(true);
    try {
      await writeClaudeMd(editorContent);
      setContent(editorContent);
      toast.success("CLAUDE.md saved successfully");
      await load();
      return true;
    } catch (e) {
      toast.error(`Failed to save CLAUDE.md: ${(e as Error).message}`);
      return false;
    } finally {
      setSaving(false);
    }
  }, [editorContent, load]);

  const reset = useCallback(() => {
    setEditorContent(content ?? "");
  }, [content]);

  useEffect(() => {
    void load();
  }, [load]);

  const hasChanges = editorContent !== (content ?? "");
  const fileExists = content !== null;

  return {
    content,
    editorContent,
    setEditorContent,
    loading,
    saving,
    hasChanges,
    fileExists,
    load,
    save,
    reset,
  };
}

/**
 * Lightweight existence probe — does NOT load content, only checks whether
 * the file is on disk. Used by the sidebar's "+ Add CLAUDE.md" vs file
 * button distinction.
 */
export function useClaudeMdExists(): boolean | null {
  const [exists, setExists] = useState<boolean | null>(null);
  useEffect(() => {
    let cancelled = false;
    void claudeMdExists()
      .then((v) => {
        if (!cancelled) setExists(v);
      })
      .catch(() => {
        if (!cancelled) setExists(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);
  return exists;
}

