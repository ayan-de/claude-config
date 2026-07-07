"use client";

import { AlertTriangle } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface Props {
  open: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function DangerousModeConfirm({ open, onConfirm, onCancel }: Props) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onCancel()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <AlertTriangle className="size-4 text-amber-500" />
            Enable dangerous mode?
          </DialogTitle>
          <DialogDescription
            render={<div />}
            className="space-y-3 pt-2 text-sm"
          >
            <p>
              Claude Code will run without asking permission for{" "}
              <strong>any</strong> file write or shell command.
            </p>
            <p>
              It will still pause for{" "}
              <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">
                rm -rf /
              </code>{" "}
              and{" "}
              <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">
                rm -rf ~
              </code>{" "}
              as a circuit breaker.
            </p>
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose render={<Button variant="ghost" />}>
            Cancel
          </DialogClose>
          <Button variant="destructive" onClick={onConfirm}>
            I understand, turn on
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
