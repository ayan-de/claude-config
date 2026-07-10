"use client";

import { Loader2, Trash2 } from "lucide-react";

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
  sessionTitle: string;
  /** null when the session has no project (unindexed). */
  projectName: string | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => Promise<void>;
  isDeleting: boolean;
}

export function SessionDeleteDialog({
  open,
  sessionTitle,
  projectName,
  onOpenChange,
  onConfirm,
  isDeleting,
}: Props) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete &ldquo;{sessionTitle}&rdquo;?</DialogTitle>
          <DialogDescription>
            {projectName
              ? `The transcript for project ${projectName} will be moved to your OS Trash. The copy on GitHub (if any) is not affected.`
              : "This unindexed transcript will be moved to your OS Trash. The copy on GitHub (if any) is not affected."}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose
            render={<Button variant="ghost" disabled={isDeleting} />}
          >
            Cancel
          </DialogClose>
          <Button
            variant="destructive"
            onClick={onConfirm}
            disabled={isDeleting}
          >
            {isDeleting ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Trash2 className="size-3.5" />
            )}
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
