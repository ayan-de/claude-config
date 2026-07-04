"use client";

import { Loader2, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface Props {
  open: boolean;
  providerName: string;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => Promise<void>;
  isDeleting: boolean;
}

export function DeleteDialog({
  open,
  providerName,
  onOpenChange,
  onConfirm,
  isDeleting,
}: Props) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete &ldquo;{providerName}&rdquo;?</DialogTitle>
          <DialogDescription>
            The provider profile and its auth token (stored in your OS
            keyring) will be permanently removed. The currently-loaded
            configuration in <code className="font-mono text-[11px]">settings.json</code>{" "}
            is not affected.
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

// Re-export so the footer can use it
import { DialogClose } from "@/components/ui/dialog";