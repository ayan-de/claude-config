"use client";

import { useEffect, useState } from "react";
import { X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { githubListLocalProjects, githubSetPathMapping } from "@/lib/api";
import { pickFolder } from "@/lib/dialogs";

interface Props {
  open: boolean;
  onClose: () => void;
  remoteOriginalPath: string;
  remoteSlug: string;
  /** Called after the mapping is persisted; parent re-triggers download. */
  onPicked: (localPath: string) => void;
}

export function ProjectPickerModal({
  open,
  onClose,
  remoteOriginalPath,
  remoteSlug,
  onPicked,
}: Props) {
  const [projects, setProjects] = useState<string[]>([]);
  const [selected, setSelected] = useState<string>("");
  const [remember, setRemember] = useState(true);

  useEffect(() => {
    if (!open) return;
    githubListLocalProjects()
      .then((p) => {
        setProjects(p);
        if (p.length > 0) setSelected(p[0]);
      })
      .catch(() => setProjects([]));
  }, [open]);

  if (!open) return null;

  async function confirm() {
    if (remember) {
      await githubSetPathMapping(remoteOriginalPath, selected, remoteSlug);
    }
    onPicked(selected);
    onClose();
  }

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-md bg-background p-4 shadow-lg">
        <header className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-semibold">Pick target project</h2>
          <Button variant="ghost" size="icon" onClick={onClose}>
            <X className="size-3.5" />
          </Button>
        </header>
        <p className="mb-3 text-[11px] text-muted-foreground">
          Remote: <code className="font-mono">{remoteOriginalPath}</code>
        </p>
        <label className="mb-1 block text-xs">Local project folder</label>
        <select
          className="w-full rounded border px-2 py-1 text-xs"
          value={selected}
          onChange={(e) => setSelected(e.target.value)}
        >
          {projects.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
        <div className="mt-3 flex items-center justify-between">
          <Button
            variant="ghost"
            size="sm"
            onClick={async () => {
              const picked = await pickFolder();
              if (picked) setSelected(picked);
            }}
          >
            Browse…
          </Button>
          <label className="flex items-center gap-1 text-[11px]">
            <input
              type="checkbox"
              checked={remember}
              onChange={(e) => setRemember(e.target.checked)}
            />
            Remember this mapping
          </label>
        </div>
        <footer className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={confirm} disabled={!selected}>
            Confirm
          </Button>
        </footer>
      </div>
    </div>
  );
}