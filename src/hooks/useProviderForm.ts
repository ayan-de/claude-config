"use client";

import { useMemo, useState } from "react";
import { deriveProviderName } from "@/lib/utils-app";
import type { Provider, ProviderInput, ProviderKind } from "@/lib/types";

interface UseProviderFormProps {
  editing: Provider | null;
  /** Kind picked in step 1 of the wizard. Locked to the editing provider's
   *  kind when editing (kind changes require delete-and-recreate). */
  kind: ProviderKind;
  onSave: (input: ProviderInput) => Promise<void>;
  isSaving: boolean;
}

/**
 * Owns all field state for the ProviderForm wizard. Which fields are
 * meaningful depends on `kind` — components should show/hide accordingly.
 *
 * The `handleSubmit` builder assembles a discriminated-union `ProviderInput`
 * whose shape matches `kind`, so the backend gets exactly the fields it
 * validates against.
 */
export function useProviderForm({
  editing,
  kind,
  onSave,
  isSaving,
}: UseProviderFormProps) {
  // -- Custom relay --
  const initialBaseUrl = editing && editing.kind === "custom" ? editing.base_url : "";
  const [baseUrl, setBaseUrl] = useState(initialBaseUrl);
  const [authToken, setAuthToken] = useState("");

  // -- Console --
  const [apiKey, setApiKey] = useState("");

  // -- Bedrock --
  const [awsRegion, setAwsRegion] = useState(
    editing && editing.kind === "bedrock" ? editing.awsRegion ?? "" : "",
  );
  const [awsProfile, setAwsProfile] = useState(
    editing && editing.kind === "bedrock" ? editing.awsProfile ?? "" : "",
  );
  const [awsAccessKeyId, setAwsAccessKeyId] = useState("");
  const [awsSecretAccessKey, setAwsSecretAccessKey] = useState("");
  const [awsSessionToken, setAwsSessionToken] = useState("");
  const [useAwsProfile, setUseAwsProfile] = useState(
    editing && editing.kind === "bedrock" ? !!editing.awsProfile : true,
  );

  // -- Vertex --
  const [vertexProjectId, setVertexProjectId] = useState(
    editing && editing.kind === "vertex" ? editing.vertexProjectId ?? "" : "",
  );
  const [vertexRegion, setVertexRegion] = useState(
    editing && editing.kind === "vertex" ? editing.vertexRegion ?? "" : "",
  );
  const [googleApplicationCredentials, setGoogleApplicationCredentials] = useState(
    editing && editing.kind === "vertex"
      ? editing.googleApplicationCredentials ?? ""
      : "",
  );

  // -- Subscription --
  const [subscriptionLabel, setSubscriptionLabel] = useState(
    editing && editing.kind === "subscription" ? editing.subscriptionLabel ?? "" : "",
  );

  // -- Model overrides + misc (all kinds) --
  const [model, setModel] = useState(editing?.model ?? "");
  const [smallFastModel, setSmallFastModel] = useState(editing?.smallFastModel ?? "");
  const [defaultSonnetModel, setDefaultSonnetModel] = useState(
    editing?.defaultSonnetModel ?? "",
  );
  const [defaultOpusModel, setDefaultOpusModel] = useState(
    editing?.defaultOpusModel ?? "",
  );
  const [defaultHaikuModel, setDefaultHaikuModel] = useState(
    editing?.defaultHaikuModel ?? "",
  );
  const [apiTimeoutMs, setApiTimeoutMs] = useState<string>(
    editing?.apiTimeoutMs?.toString() ?? "",
  );
  const [disableNonessentialTraffic, setDisableNonessentialTraffic] = useState(
    editing?.disableNonessentialTraffic ?? false,
  );

  const [showSecret, setShowSecret] = useState(false);

  // Auto-derive display name for Custom (host-based), otherwise pick a
  // reasonable default per kind. Preserved verbatim when editing.
  const derivedName = useMemo(() => {
    if (editing) return editing.name;
    switch (kind) {
      case "custom":
        return deriveProviderName(baseUrl);
      case "console":
        return "Anthropic Console";
      case "bedrock":
        return awsRegion ? `Bedrock (${awsRegion})` : "Amazon Bedrock";
      case "vertex":
        return vertexProjectId
          ? `Vertex (${vertexProjectId})`
          : "Google Vertex AI";
      case "subscription":
        // Real name is decided server-side by import_current_subscription_cmd.
        return subscriptionLabel
          ? `Subscription (${subscriptionLabel})`
          : "Subscription";
    }
  }, [editing, kind, baseUrl, awsRegion, vertexProjectId, subscriptionLabel]);

  const urlError =
    kind === "custom" && baseUrl.trim()
      ? (() => {
          try {
            const u = new URL(baseUrl);
            if (u.protocol !== "http:" && u.protocol !== "https:") {
              return "Must use http or https";
            }
            return null;
          } catch {
            return "Not a valid URL";
          }
        })()
      : null;

  const secretError = (() => {
    if (editing) return null; // secret optional on edit for every kind
    switch (kind) {
      case "custom":
        return authToken.trim() ? null : "Required when creating";
      case "console":
        return apiKey.trim() ? null : "Required when creating";
      case "bedrock":
        if (useAwsProfile) {
          return awsProfile.trim() ? null : "Profile name required";
        }
        return awsAccessKeyId.trim() && awsSecretAccessKey.trim()
          ? null
          : "Access key ID and secret access key required";
      case "vertex":
      case "subscription":
        return null;
    }
  })();

  const timeoutError = apiTimeoutMs
    ? Number.isNaN(parseInt(apiTimeoutMs, 10)) || parseInt(apiTimeoutMs, 10) <= 0
      ? "Must be a positive integer"
      : null
    : null;

  const kindSpecificReady = (() => {
    switch (kind) {
      case "custom":
        return !!baseUrl.trim();
      case "console":
      case "subscription":
        return true;
      case "bedrock":
        return !!awsRegion.trim();
      case "vertex":
        return !!vertexProjectId.trim() && !!vertexRegion.trim();
    }
  })();

  const hasErrors = !!(urlError || secretError || timeoutError);
  const canSubmit =
    !isSaving && !hasErrors && !!derivedName && kindSpecificReady;

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!canSubmit) return;

    // Common base — shared fields for every kind.
    const base = {
      id: editing?.id,
      name: derivedName,
      model: model.trim() || undefined,
      smallFastModel: smallFastModel.trim() || undefined,
      defaultSonnetModel: defaultSonnetModel.trim() || undefined,
      defaultOpusModel: defaultOpusModel.trim() || undefined,
      defaultHaikuModel: defaultHaikuModel.trim() || undefined,
      apiTimeoutMs: apiTimeoutMs ? parseInt(apiTimeoutMs, 10) : undefined,
      disableNonessentialTraffic,
    } as const;

    let payload: ProviderInput;
    switch (kind) {
      case "custom":
        payload = {
          ...base,
          kind: "custom",
          base_url: baseUrl.trim(),
          auth_token: authToken.trim() || undefined,
        };
        break;
      case "console":
        payload = {
          ...base,
          kind: "console",
          api_key: apiKey.trim() || undefined,
        };
        break;
      case "bedrock":
        payload = {
          ...base,
          kind: "bedrock",
          aws_region: awsRegion.trim(),
          aws_profile: useAwsProfile ? awsProfile.trim() || undefined : undefined,
          aws_access_key_id: !useAwsProfile
            ? awsAccessKeyId.trim() || undefined
            : undefined,
          aws_secret_access_key: !useAwsProfile
            ? awsSecretAccessKey.trim() || undefined
            : undefined,
          aws_session_token: !useAwsProfile
            ? awsSessionToken.trim() || undefined
            : undefined,
        };
        break;
      case "vertex":
        payload = {
          ...base,
          kind: "vertex",
          vertex_project_id: vertexProjectId.trim(),
          vertex_region: vertexRegion.trim(),
          google_application_credentials:
            googleApplicationCredentials.trim() || undefined,
        };
        break;
      case "subscription":
        payload = {
          ...base,
          kind: "subscription",
          subscription_label: subscriptionLabel.trim() || undefined,
        };
        break;
    }

    await onSave(payload);
  }

  return {
    // Custom
    baseUrl,
    setBaseUrl,
    authToken,
    setAuthToken,
    // Console
    apiKey,
    setApiKey,
    // Bedrock
    awsRegion,
    setAwsRegion,
    awsProfile,
    setAwsProfile,
    awsAccessKeyId,
    setAwsAccessKeyId,
    awsSecretAccessKey,
    setAwsSecretAccessKey,
    awsSessionToken,
    setAwsSessionToken,
    useAwsProfile,
    setUseAwsProfile,
    // Vertex
    vertexProjectId,
    setVertexProjectId,
    vertexRegion,
    setVertexRegion,
    googleApplicationCredentials,
    setGoogleApplicationCredentials,
    // Subscription
    subscriptionLabel,
    setSubscriptionLabel,
    // Models + misc
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
    showSecret,
    setShowSecret,
    // Derived
    derivedName,
    urlError,
    secretError,
    timeoutError,
    hasErrors,
    canSubmit,
    handleSubmit,
  };
}
