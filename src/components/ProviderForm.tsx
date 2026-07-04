"use client";

import { Eye, EyeOff, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { maskToken as appMaskToken } from "@/lib/utils-app";
import type { Provider, ProviderInput } from "@/lib/types";
import { useProviderForm } from "@/hooks/useProviderForm";

interface Props {
  /** The provider being edited; null when creating new. */
  editing: Provider | null;
  /** True if the form has unsaved changes (for Cancel confirm). */
  onCancel: () => void;
  /** Called with the derived input — name is set automatically from baseUrl. */
  onSave: (input: ProviderInput) => Promise<void>;
  isSaving: boolean;
}

export function ProviderForm({
  editing,
  onCancel,
  onSave,
  isSaving,
}: Props) {
  const {
    baseUrl,
    setBaseUrl,
    authToken,
    setAuthToken,
    model,
    setModel,
    smallFastModel,
    setSmallFastModel,
    defaultSonnetModel,
    setDefaultSonnetModel,
    defaultOpusModel,
    setDefaultOpusModel,
    defaultHaikuModel,
    setDefaultHaikuModel,
    apiTimeoutMs,
    setApiTimeoutMs,
    disableNonessentialTraffic,
    setDisableNonessentialTraffic,
    showToken,
    setShowToken,
    derivedName,
    urlError,
    tokenError,
    timeoutError,
    canSubmit,
    handleSubmit,
  } = useProviderForm({ editing, onSave, isSaving });

  return (
    <Card className="border-border/60">
      <CardHeader className="pb-3">
        <CardTitle className="text-base">
          {editing ? `Edit “${editing.name}”` : "New Provider"}
        </CardTitle>
      </CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} className="space-y-5">
          {/* Required fields */}
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label htmlFor="baseUrl">Base URL</Label>
              <Input
                id="baseUrl"
                value={baseUrl}
                onChange={(e) => setBaseUrl(e.target.value)}
                placeholder="https://api.example.com"
                className={cn(urlError && "border-destructive")}
              />
              {urlError ? (
                <p className="text-xs text-destructive">{urlError}</p>
              ) : (
                <p className="text-[10px] text-muted-foreground">
                  Provider name:{" "}
                  <span className="font-mono font-medium text-foreground">
                    {derivedName || "—"}
                  </span>
                </p>
              )}
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="authToken">Auth Token</Label>
              <div className="relative">
                <Input
                  id="authToken"
                  type={showToken ? "text" : "password"}
                  value={authToken}
                  onChange={(e) => setAuthToken(e.target.value)}
                  placeholder={
                    editing
                      ? `${appMaskToken("sk-cp-placeholder-1234abcd")} — enter to change`
                      : "sk-cp-..."
                  }
                  className={cn(
                    "pr-10 font-mono",
                    tokenError && "border-destructive",
                  )}
                  autoComplete="off"
                  spellCheck={false}
                />
                <button
                  type="button"
                  onClick={() => setShowToken((s) => !s)}
                  className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-muted-foreground hover:text-foreground"
                  aria-label={showToken ? "Hide token" : "Show token"}
                >
                  {showToken ? (
                    <EyeOff className="size-3.5" />
                  ) : (
                    <Eye className="size-3.5" />
                  )}
                </button>
              </div>
              {tokenError && (
                <p className="text-xs text-destructive">{tokenError}</p>
              )}
              <p className="text-[10px] text-muted-foreground">
                Stored in OS keyring. Never written to disk in plaintext.
              </p>
            </div>
          </div>

          <Separator />

          {/* Models */}
          <div className="space-y-3">
            <div>
              <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                Models (optional)
              </h3>
              <p className="mt-0.5 text-[10px] text-muted-foreground">
                Override Claude Code&apos;s default model selection.
              </p>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1.5">
                <Label htmlFor="model" className="text-xs">
                  Default
                </Label>
                <Input
                  id="model"
                  value={model}
                  onChange={(e) => setModel(e.target.value)}
                  placeholder="claude-sonnet-4-6"
                  className="font-mono text-xs"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="smallFastModel" className="text-xs">
                  Small / fast
                </Label>
                <Input
                  id="smallFastModel"
                  value={smallFastModel}
                  onChange={(e) => setSmallFastModel(e.target.value)}
                  placeholder="claude-haiku-4-5"
                  className="font-mono text-xs"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="defaultSonnetModel" className="text-xs">
                  Sonnet override
                </Label>
                <Input
                  id="defaultSonnetModel"
                  value={defaultSonnetModel}
                  onChange={(e) => setDefaultSonnetModel(e.target.value)}
                  className="font-mono text-xs"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="defaultOpusModel" className="text-xs">
                  Opus override
                </Label>
                <Input
                  id="defaultOpusModel"
                  value={defaultOpusModel}
                  onChange={(e) => setDefaultOpusModel(e.target.value)}
                  className="font-mono text-xs"
                />
              </div>
              <div className="col-span-2 space-y-1.5">
                <Label htmlFor="defaultHaikuModel" className="text-xs">
                  Haiku override
                </Label>
                <Input
                  id="defaultHaikuModel"
                  value={defaultHaikuModel}
                  onChange={(e) => setDefaultHaikuModel(e.target.value)}
                  className="font-mono text-xs"
                />
              </div>
            </div>
          </div>

          <Separator />

          {/* Advanced */}
          <div className="space-y-3">
            <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              Advanced
            </h3>
            <div className="space-y-1.5">
              <Label htmlFor="apiTimeoutMs" className="text-xs">
                API timeout (ms)
              </Label>
              <Input
                id="apiTimeoutMs"
                value={apiTimeoutMs}
                onChange={(e) => setApiTimeoutMs(e.target.value)}
                placeholder="3000000"
                className={cn("font-mono text-xs", timeoutError && "border-destructive")}
                inputMode="numeric"
              />
              {timeoutError && (
                <p className="text-xs text-destructive">{timeoutError}</p>
              )}
            </div>
            <label className="flex cursor-pointer items-start gap-2.5 rounded-md border bg-muted/20 p-2.5">
              <input
                type="checkbox"
                checked={disableNonessentialTraffic}
                onChange={(e) =>
                  setDisableNonessentialTraffic(e.target.checked)
                }
                className="mt-0.5 size-3.5 accent-foreground"
              />
              <div className="space-y-0.5">
                <span className="text-xs font-medium">
                  Block non-essential traffic
                </span>
                <p className="text-[10px] text-muted-foreground">
                  Sets CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1
                </p>
              </div>
            </label>
          </div>

          <div className="flex items-center justify-end gap-2 pt-2">
            <Button type="button" variant="ghost" onClick={onCancel}>
              Cancel
            </Button>
            <Button type="submit" disabled={!canSubmit}>
              {isSaving && <Loader2 className="size-3.5 animate-spin" />}
              {editing ? "Save changes" : "Create provider"}
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  );
}