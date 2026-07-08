"use client";

import { useRef, useState } from "react";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Eye,
  EyeOff,
  ExternalLink,
  KeyRound,
  Loader2,
  Lock,
  Plus,
  Settings2,
  Sparkles,
  Trash2,
  Upload,
} from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ProviderLogo } from "@/components/ProviderLogo";
import { LoopVideo } from "@/components/LoopVideo";
import { TrackerTab } from "@/components/TrackerTab";
import { cn } from "@/lib/utils";
import { kindLabel, maskToken as appMaskToken } from "@/lib/utils-app";
import {
  CUSTOM_SENTINEL,
  PRESET_PROVIDERS,
  getPresetApiKeyUrl,
} from "@/lib/presetProviders";
import { importCurrentSubscription } from "@/lib/api";
import type { Provider, ProviderInput, ProviderKind } from "@/lib/types";
import { useProviderForm } from "@/hooks/useProviderForm";

/**
 * The two tabs in the ProviderForm. "Tracker" only renders when
 * `editing` is non-null — the tracker is per-existing-provider, so
 * there's nothing to track during the create flow.
 */
type ProviderFormTab = "configuration" | "tracker";

interface Props {
  editing: Provider | null;
  onCancel: () => void;
  onSave: (input: ProviderInput) => Promise<void>;
  /** Called when the user opts to import a Subscription via
   *  `claude /login` snapshot. */
  onSubscriptionImported: (p: Provider) => void;
  /** Only invoked in edit mode — opens the delete-confirmation dialog. */
  onDelete: () => void;
  isSaving: boolean;
}

const KIND_OPTIONS: {
  kind: ProviderKind;
  title: string;
  description: string;
}[] = [
  {
    kind: "subscription",
    title: "Subscription",
    description: "Claude account: Pro, Max, Team, or Enterprise",
  },
  {
    kind: "console",
    title: "Anthropic Console",
    description: "API key billing via console.anthropic.com",
  },
  {
    kind: "custom",
    title: "Custom relay",
    description: "Third-party proxy exposing the Anthropic API",
  },
  {
    kind: "bedrock",
    title: "Amazon Bedrock",
    description: "AWS Bedrock via CLAUDE_CODE_USE_BEDROCK",
  },
  {
    kind: "vertex",
    title: "Google Vertex AI",
    description: "Vertex AI via CLAUDE_CODE_USE_VERTEX",
  },
];

export function ProviderForm({
  editing,
  onCancel,
  onSave,
  onSubscriptionImported,
  onDelete,
  isSaving,
}: Props) {
  // When editing, the kind is locked to the existing provider's kind (the
  // backend rejects kind changes on update). Otherwise we start on step 1
  // (kind picker) with no selection.
  const [selectedKind, setSelectedKind] = useState<ProviderKind | null>(
    editing?.kind ?? null,
  );
  // Tab state — only meaningful when editing. Resets to "configuration"
  // whenever the editing target changes (key on <ProviderForm> in Main
  // already handles that, but we also reset on kind changes for safety).
  const [tab, setTab] = useState<ProviderFormTab>("configuration");
  const [importing, setImporting] = useState(false);
  const [modelsExpanded, setModelsExpanded] = useState(() => {
    if (!editing) return false;
    return !!(
      editing.model ||
      editing.smallFastModel ||
      editing.defaultSonnetModel ||
      editing.defaultOpusModel ||
      editing.defaultHaikuModel
    );
  });
  const [advancedExpanded, setAdvancedExpanded] = useState(() => {
    if (!editing) return false;
    return !!(editing.apiTimeoutMs || editing.disableNonessentialTraffic);
  });

  // Kind picker (step 1)
  if (!selectedKind) {
    return (
      <Card className="border-border/60">
        <CardHeader className="pb-3">
          <div className="flex items-center gap-5">
            <div className="flex-1 min-w-0">
              <CardTitle className="text-base">New Provider</CardTitle>
              <p className="text-xs text-muted-foreground mt-1">
                How do you want Claude Code to authenticate?
              </p>
            </div>
            <div className="shrink-0 overflow-hidden rounded-lg bg-muted/10 p-2">
              <LoopVideo
                src="/clawd-laptop.webm"
                className="h-20 w-auto dark:invert-0 dark:hue-rotate-0 dark:mix-blend-screen invert hue-rotate-180 mix-blend-multiply"
              />
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div className="space-y-2">
            {KIND_OPTIONS.map((opt) => (
              <button
                key={opt.kind}
                type="button"
                onClick={() => setSelectedKind(opt.kind)}
                className="group flex w-full items-center justify-between gap-3 rounded-lg border bg-card/60 p-3 text-left transition-colors hover:border-foreground/30 hover:bg-card/90 cursor-pointer"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-semibold">{opt.title}</span>
                    <span className="rounded-full border border-border/60 bg-muted/30 px-1.5 py-0.5 text-[9px] uppercase tracking-wider text-muted-foreground/80">
                      {kindLabel(opt.kind)}
                    </span>
                  </div>
                  <p className="mt-0.5 text-[11px] text-muted-foreground">
                    {opt.description}
                  </p>
                </div>
                <ChevronRight className="size-4 text-muted-foreground group-hover:text-foreground shrink-0" />
              </button>
            ))}
          </div>
          <div className="flex items-center justify-end pt-4">
            <Button type="button" variant="ghost" onClick={onCancel}>
              Cancel
            </Button>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <KindForm
      editing={editing}
      kind={selectedKind}
      importing={importing}
      setImporting={setImporting}
      onSubscriptionImported={onSubscriptionImported}
      onDelete={onDelete}
      modelsExpanded={modelsExpanded}
      setModelsExpanded={setModelsExpanded}
      advancedExpanded={advancedExpanded}
      setAdvancedExpanded={setAdvancedExpanded}
      onBack={editing ? null : () => setSelectedKind(null)}
      onCancel={onCancel}
      onSave={onSave}
      isSaving={isSaving}
      tab={tab}
      setTab={setTab}
    />
  );
}

interface KindFormProps {
  editing: Provider | null;
  kind: ProviderKind;
  importing: boolean;
  setImporting: (b: boolean) => void;
  onSubscriptionImported: (p: Provider) => void;
  onDelete: () => void;
  modelsExpanded: boolean;
  setModelsExpanded: (b: boolean) => void;
  advancedExpanded: boolean;
  setAdvancedExpanded: (b: boolean) => void;
  onBack: (() => void) | null;
  onCancel: () => void;
  onSave: (input: ProviderInput) => Promise<void>;
  isSaving: boolean;
  tab: ProviderFormTab;
  setTab: (t: ProviderFormTab) => void;
}

function KindForm({
  editing,
  kind,
  importing,
  setImporting,
  onSubscriptionImported,
  onDelete,
  modelsExpanded,
  setModelsExpanded,
  advancedExpanded,
  setAdvancedExpanded,
  onBack,
  onCancel,
  onSave,
  isSaving,
  tab,
  setTab,
}: KindFormProps) {
  const f = useProviderForm({ editing, kind, onSave, isSaving });

  async function handleImportSubscription() {
    setImporting(true);
    try {
      const p = await importCurrentSubscription(
        f.subscriptionLabel.trim() || undefined,
      );
      toast.success(`Imported “${p.name}”`);
      onSubscriptionImported(p);
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setImporting(false);
    }
  }

  return (
    <Card className="border-border/60">
      <CardHeader className="pb-3">
        {editing ? (
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2 min-w-0">
              {onBack && (
                <button
                  type="button"
                  onClick={onBack}
                  className="text-muted-foreground hover:text-foreground cursor-pointer"
                  aria-label="Back"
                >
                  <ChevronLeft className="size-4" />
                </button>
              )}
              <div className="relative size-8 rounded-lg border bg-muted/20 flex items-center justify-center shrink-0 overflow-hidden">
                <ProviderLogo
                  svg={editing.logoSvg}
                  size={20}
                  className="rounded"
                />
              </div>
              <CardTitle className="text-base truncate">
                {editing.name}
              </CardTitle>
            </div>
            <div className="flex items-center gap-1">
              <TabButton
                active={tab === "configuration"}
                onClick={() => setTab("configuration")}
                icon={<Settings2 className="size-3.5" />}
                label="Configuration"
              />
              <TabButton
                active={tab === "tracker"}
                onClick={() => setTab("tracker")}
                icon={<Sparkles className="size-3.5" />}
                label="Tracker"
              />
            </div>
          </div>
        ) : (
          <div className="flex items-center gap-5">
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2">
                {onBack && (
                  <button
                    type="button"
                    onClick={onBack}
                    className="text-muted-foreground hover:text-foreground cursor-pointer"
                    aria-label="Back"
                  >
                    <ChevronLeft className="size-4" />
                  </button>
                )}
                <CardTitle className="text-base">
                  New {kindLabel(kind)} provider
                </CardTitle>
              </div>
            </div>
            <div className="shrink-0 overflow-hidden rounded-lg bg-muted/10 p-2">
              <LoopVideo
                src="/clawd-laptop.webm"
                className="h-20 w-auto dark:invert-0 dark:hue-rotate-0 dark:mix-blend-screen invert hue-rotate-180 mix-blend-multiply"
              />
            </div>
          </div>
        )}
      </CardHeader>
      <CardContent>
        {editing && tab === "tracker" ? (
          <TrackerTab
            providerId={editing.id}
            providerName={editing.name}
            providerKind={editing.kind}
            providerBaseUrl={
              editing.kind === "custom" ? editing.base_url : null
            }
          />
        ) : (
        <form onSubmit={f.handleSubmit} className="space-y-5">
          {/* ---------- Kind-specific fields ---------- */}

          {kind === "subscription" && !editing && (
            <div className="space-y-3 rounded-lg border bg-muted/20 p-4">
              <p className="text-xs text-muted-foreground">
                Run <code className="font-mono">claude /login</code> in a
                terminal to complete OAuth, then import the current session
                below. Add a label to distinguish this from other subscription
                accounts.
              </p>
              <div className="space-y-1.5">
                <Label htmlFor="subLabel" className="text-xs">
                  Label (optional)
                </Label>
                <Input
                  id="subLabel"
                  value={f.subscriptionLabel}
                  onChange={(e) => f.setSubscriptionLabel(e.target.value)}
                  placeholder="Personal Max, Work Pro, …"
                />
              </div>
              <Button
                type="button"
                onClick={handleImportSubscription}
                disabled={importing}
                variant="secondary"
                className="w-full"
              >
                {importing ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <KeyRound className="size-3.5" />
                )}
                Import current claude /login session
              </Button>
            </div>
          )}

          {kind === "subscription" && editing && (
            <div className="space-y-1.5">
              <Label htmlFor="subLabel" className="text-xs">
                Label
              </Label>
              <Input
                id="subLabel"
                value={f.subscriptionLabel}
                onChange={(e) => f.setSubscriptionLabel(e.target.value)}
              />
            </div>
          )}

          {kind === "console" && (
            <div className="space-y-1.5">
              <Label htmlFor="apiKey">API key</Label>
              <SecretInput
                id="apiKey"
                value={f.apiKey}
                onChange={f.setApiKey}
                show={f.showSecret}
                setShow={f.setShowSecret}
                placeholder={
                  editing
                    ? `${appMaskToken("sk-ant-placeholder-1234abcd")} — enter to change`
                    : "sk-ant-..."
                }
                error={f.secretError}
              />
              <p className="text-[10px] text-muted-foreground">
                Sets <code className="font-mono">ANTHROPIC_API_KEY</code>.
                Stored in OS keyring.
              </p>
            </div>
          )}

          {kind === "custom" && (
            <CustomKindFields editing={editing} f={f} />
          )}

          {kind === "bedrock" && (
            <div className="space-y-3">
              <div className="space-y-1.5">
                <Label htmlFor="awsRegion">AWS region</Label>
                <Input
                  id="awsRegion"
                  value={f.awsRegion}
                  onChange={(e) => f.setAwsRegion(e.target.value)}
                  placeholder="us-east-1"
                  className="font-mono text-xs"
                />
              </div>
              <div className="flex gap-2 rounded-md border p-2 text-xs">
                <label className="flex items-center gap-1.5 cursor-pointer">
                  <input
                    type="radio"
                    checked={f.useAwsProfile}
                    onChange={() => f.setUseAwsProfile(true)}
                    className="size-3.5 accent-foreground"
                  />
                  Use AWS profile
                </label>
                <label className="flex items-center gap-1.5 cursor-pointer">
                  <input
                    type="radio"
                    checked={!f.useAwsProfile}
                    onChange={() => f.setUseAwsProfile(false)}
                    className="size-3.5 accent-foreground"
                  />
                  Static credentials
                </label>
              </div>
              {f.useAwsProfile ? (
                <div className="space-y-1.5">
                  <Label htmlFor="awsProfile">Profile name</Label>
                  <Input
                    id="awsProfile"
                    value={f.awsProfile}
                    onChange={(e) => f.setAwsProfile(e.target.value)}
                    placeholder="default"
                  />
                  <p className="text-[10px] text-muted-foreground">
                    Reads credentials from{" "}
                    <code className="font-mono">~/.aws/credentials</code>.
                  </p>
                </div>
              ) : (
                <div className="space-y-2">
                  <div className="space-y-1.5">
                    <Label htmlFor="awsKey">Access key ID</Label>
                    <Input
                      id="awsKey"
                      value={f.awsAccessKeyId}
                      onChange={(e) => f.setAwsAccessKeyId(e.target.value)}
                      placeholder={editing && !f.useAwsProfile ? "Saved — enter to change" : "AKIA..."}
                      className="font-mono text-xs"
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="awsSecret">Secret access key</Label>
                    <SecretInput
                      id="awsSecret"
                      value={f.awsSecretAccessKey}
                      onChange={f.setAwsSecretAccessKey}
                      show={f.showSecret}
                      setShow={f.setShowSecret}
                      placeholder={editing && !f.useAwsProfile ? "Saved — enter to change" : "Secret access key"}
                      error={f.secretError}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="awsSess" className="text-xs">
                      Session token (optional)
                    </Label>
                    <Input
                      id="awsSess"
                      value={f.awsSessionToken}
                      onChange={(e) => f.setAwsSessionToken(e.target.value)}
                      placeholder="STS session token, if using SSO/AssumeRole"
                      className="font-mono text-xs"
                    />
                  </div>
                </div>
              )}
            </div>
          )}

          {kind === "vertex" && (
            <div className="space-y-3">
              <div className="space-y-1.5">
                <Label htmlFor="vertexProject">Project ID</Label>
                <Input
                  id="vertexProject"
                  value={f.vertexProjectId}
                  onChange={(e) => f.setVertexProjectId(e.target.value)}
                  placeholder="my-gcp-project"
                  className="font-mono text-xs"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="vertexRegion">Region</Label>
                <Input
                  id="vertexRegion"
                  value={f.vertexRegion}
                  onChange={(e) => f.setVertexRegion(e.target.value)}
                  placeholder="us-central1"
                  className="font-mono text-xs"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="gcpCreds" className="text-xs">
                  Service account key path (optional)
                </Label>
                <Input
                  id="gcpCreds"
                  value={f.googleApplicationCredentials}
                  onChange={(e) =>
                    f.setGoogleApplicationCredentials(e.target.value)
                  }
                  placeholder="/path/to/service-account.json"
                  className="font-mono text-xs"
                />
                <p className="text-[10px] text-muted-foreground">
                  Leave blank to use gcloud Application Default Credentials.
                </p>
              </div>
            </div>
          )}

          <Separator />

          {/* ---------- Model overrides (all kinds) ---------- */}
          <div className="space-y-3">
            <button
              type="button"
              onClick={() => setModelsExpanded(!modelsExpanded)}
              className="flex w-full items-center justify-between text-left group focus:outline-none cursor-pointer"
            >
              <div>
                <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground group-hover:text-foreground transition-colors flex items-center gap-1.5">
                  Models (optional)
                  {modelsExpanded ? (
                    <ChevronDown className="size-3.5" />
                  ) : (
                    <ChevronRight className="size-3.5" />
                  )}
                </h3>
                <p className="mt-0.5 text-[10px] text-muted-foreground">
                  Override Claude Code&apos;s default model selection.
                </p>
              </div>
            </button>

            {modelsExpanded && (
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3 pt-1">
                <div className="space-y-1.5">
                  <Label htmlFor="model" className="text-xs">
                    Default
                  </Label>
                  <Input
                    id="model"
                    value={f.model}
                    onChange={(e) => f.setModel(e.target.value)}
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
                    value={f.smallFastModel}
                    onChange={(e) => f.setSmallFastModel(e.target.value)}
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
                    value={f.defaultSonnetModel}
                    onChange={(e) => f.setDefaultSonnetModel(e.target.value)}
                    className="font-mono text-xs"
                  />
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor="defaultOpusModel" className="text-xs">
                    Opus override
                  </Label>
                  <Input
                    id="defaultOpusModel"
                    value={f.defaultOpusModel}
                    onChange={(e) => f.setDefaultOpusModel(e.target.value)}
                    className="font-mono text-xs"
                  />
                </div>
                <div className="col-span-1 md:col-span-2 space-y-1.5">
                  <Label htmlFor="defaultHaikuModel" className="text-xs">
                    Haiku override
                  </Label>
                  <Input
                    id="defaultHaikuModel"
                    value={f.defaultHaikuModel}
                    onChange={(e) => f.setDefaultHaikuModel(e.target.value)}
                    className="font-mono text-xs"
                  />
                </div>
              </div>
            )}
          </div>

          <Separator />

          {/* ---------- Advanced (all kinds) ---------- */}
          <div className="space-y-3">
            <button
              type="button"
              onClick={() => setAdvancedExpanded(!advancedExpanded)}
              className="flex w-full items-center justify-between text-left group focus:outline-none cursor-pointer"
            >
              <div>
                <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground group-hover:text-foreground transition-colors flex items-center gap-1.5">
                  Advanced
                  {advancedExpanded ? (
                    <ChevronDown className="size-3.5" />
                  ) : (
                    <ChevronRight className="size-3.5" />
                  )}
                </h3>
              </div>
            </button>

            {advancedExpanded && (
              <div className="space-y-3 pt-1">
                <div className="space-y-1.5">
                  <Label htmlFor="apiTimeoutMs" className="text-xs">
                    API timeout (ms)
                  </Label>
                  <Input
                    id="apiTimeoutMs"
                    value={f.apiTimeoutMs}
                    onChange={(e) => f.setApiTimeoutMs(e.target.value)}
                    placeholder="3000000"
                    className={cn(
                      "font-mono text-xs",
                      f.timeoutError && "border-destructive",
                    )}
                    inputMode="numeric"
                  />
                  {f.timeoutError && (
                    <p className="text-xs text-destructive">{f.timeoutError}</p>
                  )}
                </div>
                <label className="flex cursor-pointer items-start gap-2.5 rounded-md border bg-muted/20 p-2.5">
                  <input
                    type="checkbox"
                    checked={f.disableNonessentialTraffic}
                    onChange={(e) =>
                      f.setDisableNonessentialTraffic(e.target.checked)
                    }
                    className="mt-0.5 size-3.5 accent-foreground"
                  />
                  <div className="space-y-0.5">
                    <span className="text-xs font-medium">
                      Block non-essential traffic
                    </span>
                    <p className="text-[10px] text-muted-foreground">
                      Sets{" "}
                      <code className="font-mono">
                        CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1
                      </code>
                    </p>
                  </div>
                </label>
              </div>
            )}
          </div>

          <div className="flex items-center justify-between gap-2 pt-2">
            {editing ? (
              <Button
                type="button"
                variant="ghost"
                onClick={onDelete}
                className="text-muted-foreground hover:text-destructive hover:bg-destructive/10 cursor-pointer"
              >
                <Trash2 className="size-3.5" />
                Delete
              </Button>
            ) : (
              <span />
            )}
            <div className="flex items-center gap-2">
              <Button type="button" variant="ghost" onClick={onCancel}>
                Cancel
              </Button>
              {/* Subscription create is completed via the import button above;
                  only surface the primary submit for other kinds or when editing. */}
              {!(kind === "subscription" && !editing) && (
                <Button type="submit" disabled={!f.canSubmit}>
                  {isSaving && <Loader2 className="size-3.5 animate-spin" />}
                  {editing ? "Save changes" : "Create provider"}
                </Button>
              )}
            </div>
          </div>
        </form>
        )}
      </CardContent>
    </Card>
  );
}

interface SecretInputProps {
  id: string;
  value: string;
  onChange: (v: string) => void;
  show: boolean;
  setShow: (b: boolean) => void;
  placeholder?: string;
  error: string | null;
}

/**
 * Sub-tab bar inside the ProviderForm Card. Only rendered when `editing`
 * is set — the create flow has no tracker to manage. The "Close" button
 * is intentionally outside the tabs so the user always has a way to
 * dismiss the modal, regardless of which tab is active.
 */
function TabButton({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "inline-flex items-center gap-1.5 rounded-t-md border-b-2 px-3 py-1.5 text-xs font-medium transition-colors cursor-pointer",
        active
          ? "border-primary text-foreground"
          : "border-transparent text-muted-foreground hover:text-foreground hover:border-border",
      )}
      aria-pressed={active}
    >
      {icon}
      {label}
    </button>
  );
}

function SecretInput({
  id,
  value,
  onChange,
  show,
  setShow,
  placeholder,
  error,
}: SecretInputProps) {
  return (
    <>
      <div className="relative">
        <Input
          id={id}
          type={show ? "text" : "password"}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          className={cn("pr-10 font-mono", error && "border-destructive")}
          autoComplete="off"
          spellCheck={false}
        />
        <button
          type="button"
          onClick={() => setShow(!show)}
          className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-muted-foreground hover:text-foreground"
          aria-label={show ? "Hide secret" : "Show secret"}
        >
          {show ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
        </button>
      </div>
      {error && <p className="text-xs text-destructive">{error}</p>}
    </>
  );
}

interface CustomKindFieldsProps {
  editing: Provider | null;
  f: ReturnType<typeof useProviderForm>;
}

/**
 * Custom-relay form body: preset dropdown (create-only), logo upload, base
 * URL, auth token. On edit, the preset is locked to whatever the existing
 * provider was created as — the user can change name/baseUrl/auth token
 * but not the template, mirroring how `kind` itself is locked.
 */
function CustomKindFields({ editing, f }: CustomKindFieldsProps) {
  const fileInputRef = useRef<HTMLInputElement>(null);

  // Edit mode: render the locked preset as a static row with the existing
  // logo (no dropdown, no upload button). Changing template requires a
  // delete-and-recreate — consistent with how `kind` is locked today.
  if (editing) {
    const preset = PRESET_PROVIDERS.find((p) => p.id === f.selectedPresetId);
    const label = preset?.name ?? "Custom";
    const apiKeyUrl = getPresetApiKeyUrl(f.selectedPresetId);
    return (
      <div className="space-y-4">
        <div className="flex items-center gap-2.5 rounded-lg border bg-muted/20 p-2.5">
          <ProviderLogo svg={editing.logoSvg} size={28} className="rounded" />
          <div className="min-w-0 flex-1">
            <p className="text-xs font-medium leading-none">{label}</p>
            <p className="mt-1 text-[10px] text-muted-foreground">
              <Lock className="inline size-2.5 -translate-y-px" /> Template
              locked — delete and recreate to change
            </p>
          </div>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="baseUrl">Base URL</Label>
          <Input
            id="baseUrl"
            value={f.baseUrl}
            onChange={(e) => f.setBaseUrl(e.target.value)}
            placeholder="https://api.example.com"
            className={cn(f.urlError && "border-destructive")}
          />
          {f.urlError && (
            <p className="text-xs text-destructive">{f.urlError}</p>
          )}
        </div>

        {/* Auth token (full width) */}
        <div className="space-y-1.5">
          <Label htmlFor="authToken">Auth token</Label>
          {apiKeyUrl && <PresetApiKeyHint url={apiKeyUrl} />}
          <SecretInput
            id="authToken"
            value={f.authToken}
            onChange={f.setAuthToken}
            show={f.showSecret}
            setShow={f.setShowSecret}
            placeholder={`${appMaskToken("sk-cp-placeholder-1234abcd")} — enter to change`}
            error={f.secretError}
          />
          <p className="text-[10px] text-muted-foreground">
            Sets <code className="font-mono">ANTHROPIC_AUTH_TOKEN</code>.
            Stored in OS keyring.
          </p>
        </div>
      </div>
    );
  }

  const presetValue = f.selectedPresetId ?? CUSTOM_SENTINEL;

  return (
    <div className="space-y-3">
      {/* Preset picker */}
      <div className="space-y-1.5">
        <Label htmlFor="preset">Provider template</Label>
        <Select
          value={presetValue}
          onValueChange={(v) => {
            if (!v) return;
            if (v === CUSTOM_SENTINEL) {
              f.applyPreset(CUSTOM_SENTINEL);
              return;
            }
            void f.applyPreset(v);
          }}
        >
          <SelectTrigger id="preset" className="w-full">
            <SelectValue>
              {presetValue === CUSTOM_SENTINEL ? (
                <span className="inline-flex items-center gap-1.5">
                  <Plus className="size-3" /> Custom
                </span>
              ) : (
                PRESET_PROVIDERS.find((p) => p.id === presetValue)?.name ?? "Custom"
              )}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {PRESET_PROVIDERS.map((p) => (
              <SelectItem key={p.id} value={p.id}>
                {p.name}
              </SelectItem>
            ))}
            <SelectItem value={CUSTOM_SENTINEL}>
              <span className="inline-flex items-center gap-1.5">
                <Plus className="size-3" /> Custom
              </span>
            </SelectItem>
          </SelectContent>
        </Select>
        <p className="text-[10px] text-muted-foreground">
          Picks the name and base URL. Choose “Custom” to define your own.
        </p>
      </div>

      <div className="flex items-start gap-4">
        {/* Left column: Circular logo preview / upload */}
        <div className="shrink-0 pt-1.5 flex flex-col items-center gap-1.5">
          <div
            className={cn(
              "relative group size-16 rounded-full border flex items-center justify-center bg-muted/10 overflow-hidden transition-colors duration-150",
              f.selectedPresetId === CUSTOM_SENTINEL ? "border-dashed hover:border-muted-foreground/50" : "border-solid"
            )}
          >
            <ProviderLogo svg={f.logoSvg} size={44} className="rounded-full" />
            {f.selectedPresetId === CUSTOM_SENTINEL && (
              <>
                <input
                  ref={fileInputRef}
                  type="file"
                  accept=".svg,image/svg+xml"
                  className="hidden"
                  onChange={(e) => {
                    const file = e.target.files?.[0] ?? null;
                    f.handleLogoUpload(file);
                    e.target.value = "";
                  }}
                />
                <button
                  type="button"
                  onClick={() => fileInputRef.current?.click()}
                  className="absolute inset-0 bg-black/70 opacity-0 group-hover:opacity-100 transition-opacity flex flex-col items-center justify-center text-[10px] text-white font-medium cursor-pointer"
                >
                  <Upload className="size-4 mb-0.5" />
                  Upload
                </button>
              </>
            )}
          </div>
          {f.selectedPresetId === CUSTOM_SENTINEL && (
            <span className="text-[9px] text-muted-foreground font-medium select-none">
              {f.logoSvg ? "Custom" : "Optional"}
            </span>
          )}
        </div>

        {/* Right column: Base URL field */}
        <div className="flex-1 min-w-0">
          <div className="space-y-1.5">
            <Label htmlFor="baseUrl">Base URL</Label>
            <Input
              id="baseUrl"
              value={f.baseUrl}
              onChange={(e) => f.setBaseUrl(e.target.value)}
              placeholder="https://api.example.com"
              className={cn(f.urlError && "border-destructive")}
            />
            {f.urlError ? (
              <p className="text-xs text-destructive">{f.urlError}</p>
            ) : (
              <p className="text-[10px] text-muted-foreground">
                Provider name:{" "}
                <span className="font-mono font-medium text-foreground">
                  {f.derivedName || "—"}
                </span>
              </p>
            )}
          </div>
        </div>
      </div>

      {/* Auth token (full width) */}
      <div className="space-y-1.5">
        <Label htmlFor="authToken">Auth token</Label>
        {getPresetApiKeyUrl(f.selectedPresetId) && (
          <PresetApiKeyHint url={getPresetApiKeyUrl(f.selectedPresetId)!} />
        )}
        <SecretInput
          id="authToken"
          value={f.authToken}
          onChange={f.setAuthToken}
          show={f.showSecret}
          setShow={f.setShowSecret}
          placeholder="sk-cp-..."
          error={f.secretError}
        />
        <p className="text-[10px] text-muted-foreground">
          Sets <code className="font-mono">ANTHROPIC_AUTH_TOKEN</code>.
          Stored in OS keyring.
        </p>
      </div>
      {f.logoError && (
        <p className="text-xs text-destructive text-center mt-2">{f.logoError}</p>
      )}
    </div>
  );
}

/**
 * Inline hint under the Auth token field for presets that publish a self-serve
 * API-key dashboard. Renders the full URL so it is visible — the user pastes
 * the link into their browser themselves.
 */
function PresetApiKeyHint({ url }: { url: string }) {
  return (
    <p className="text-[10px] text-muted-foreground">
      Grab your free API key from{" "}
      <a
        href={url}
        target="_blank"
        rel="noopener noreferrer"
        className="inline-flex items-center gap-1 break-all font-mono font-medium text-foreground underline-offset-2 hover:underline"
      >
        {url}
        <ExternalLink className="size-2.5 shrink-0 -translate-y-px" />
      </a>
    </p>
  );
}
