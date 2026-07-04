"use client";

import { useMemo, useState } from "react";
import { deriveProviderName } from "@/lib/utils-app";
import type { Provider, ProviderInput } from "@/lib/types";

interface UseProviderFormProps {
  editing: Provider | null;
  onSave: (input: ProviderInput) => Promise<void>;
  isSaving: boolean;
}

export function useProviderForm({ editing, onSave, isSaving }: UseProviderFormProps) {
  const [baseUrl, setBaseUrl] = useState(editing?.baseUrl ?? "");
  const [authToken, setAuthToken] = useState("");
  const [model, setModel] = useState(editing?.model ?? "");
  const [smallFastModel, setSmallFastModel] = useState(
    editing?.smallFastModel ?? "",
  );
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
  const [disableNonessentialTraffic, setDisableNonessentialTraffic] =
    useState(editing?.disableNonessentialTraffic ?? false);
  const [showToken, setShowToken] = useState(false);

  // Name is derived from baseUrl — user never types it. When editing an
  // existing provider, we keep its original name.
  const derivedName = useMemo(() => deriveProviderName(baseUrl), [baseUrl]);
  const finalName = editing?.name ?? derivedName;

  const urlError = baseUrl.trim()
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

  const tokenError =
    !editing && !authToken.trim() ? "Required when creating a new provider" : null;

  const timeoutError = apiTimeoutMs
    ? Number.isNaN(parseInt(apiTimeoutMs, 10)) || parseInt(apiTimeoutMs, 10) <= 0
      ? "Must be a positive integer"
      : null
    : null;

  const hasErrors = !!(urlError || tokenError || timeoutError);

  const canSubmit =
    !isSaving &&
    !hasErrors &&
    finalName &&
    baseUrl.trim() &&
    (editing || authToken.trim());

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    await onSave({
      id: editing?.id,
      name: finalName,
      baseUrl: baseUrl.trim(),
      authToken: authToken.trim() || (editing ? "unchanged" : ""),
      model: model.trim() || undefined,
      smallFastModel: smallFastModel.trim() || undefined,
      defaultSonnetModel: defaultSonnetModel.trim() || undefined,
      defaultOpusModel: defaultOpusModel.trim() || undefined,
      defaultHaikuModel: defaultHaikuModel.trim() || undefined,
      apiTimeoutMs: apiTimeoutMs ? parseInt(apiTimeoutMs, 10) : undefined,
      disableNonessentialTraffic,
    });
  }

  return {
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
    finalName,
    urlError,
    tokenError,
    timeoutError,
    hasErrors,
    canSubmit,
    handleSubmit,
  };
}
