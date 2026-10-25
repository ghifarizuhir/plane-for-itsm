/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState, type FormEvent } from "react";
import { Switch } from "@makeplane/propel/components/switch";
import { AddOutline, CloseOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { IconButton } from "@plane/propel/icon-button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IMetricSource, IService } from "@plane/types";
import { CustomSearchSelect, EModalPosition, EModalWidth, Input as UIKitInput, ModalCore } from "@plane/ui";
import { cn } from "@plane/utils";
import { ServiceMetricsService } from "@/services/service-metrics.service";

const service = new ServiceMetricsService();

type Mapping = { id: string; key: string; serviceId: string };

const newMapping = (): Mapping => ({
  id: typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`,
  key: "",
  serviceId: "",
});

const SELECT_BUTTON_CLASS = "rounded-md border-[0.5px] border-subtle-1";

type Props = {
  isOpen: boolean;
  workspaceSlug: string;
  source: IMetricSource | null;
  services: IService[];
  onClose: () => void;
  onSaved: () => void | Promise<void>;
};

export function MetricSourceForm({ isOpen, workspaceSlug, source, services, onClose, onSaved }: Props) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [mappings, setMappings] = useState<Mapping[]>([newMapping()]);
  const [fallback, setFallback] = useState("");
  const [isActive, setIsActive] = useState(true);
  const [saving, setSaving] = useState(false);
  const [nameError, setNameError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    setName(source?.name ?? "");
    const entries = Object.entries(source?.config.service_map ?? {});
    const next: Mapping[] = [];
    for (const [key, serviceId] of entries) {
      const mapping = newMapping();
      mapping.key = key;
      mapping.serviceId = serviceId;
      next.push(mapping);
    }
    setMappings(next.length ? next : [newMapping()]);
    setFallback(source?.config.fallback_service_id ?? "");
    setIsActive(source?.is_active ?? true);
    setNameError(null);
    setError(null);
  }, [isOpen, source]);

  const handleClose = () => {
    if (saving) return;
    onClose();
  };

  const updateMapping = (index: number, patch: Partial<Mapping>) => {
    setMappings((current) => current.map((mapping, i) => (i === index ? { ...mapping, ...patch } : mapping)));
  };

  const removeMapping = (index: number) => {
    setMappings((current) => {
      if (current.length === 1) return [newMapping()];
      return current.filter((_, i) => i !== index);
    });
  };

  const submit = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      setNameError(t("workspace_settings.settings.metric_sources.name_required"));
      return;
    }
    const serviceMap: Record<string, string> = {};
    for (const mapping of mappings) {
      if (!mapping.key.trim() || !mapping.serviceId) continue;
      serviceMap[mapping.key.trim()] = mapping.serviceId;
    }
    const payload = {
      name: trimmed,
      is_active: isActive,
      config: { service_map: serviceMap, fallback_service_id: fallback || null },
    };
    setSaving(true);
    setNameError(null);
    setError(null);
    try {
      if (source) {
        await service.updateSource(workspaceSlug, source.id, payload);
      } else {
        await service.createSource(workspaceSlug, payload);
      }
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t(
          source
            ? "workspace_settings.settings.metric_sources.updated"
            : "workspace_settings.settings.metric_sources.created"
        ),
      });
      await onSaved();
    } catch (err) {
      setError((err as { response?: { data?: { error?: string } } })?.response?.data?.error ?? t("common.error"));
    } finally {
      setSaving(false);
    }
  };

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void submit();
  };

  const selectedFallback = services.find((item) => item.id === fallback);
  const serviceOptions = services.map((item) => ({
    value: item.id,
    query: item.name,
    content: <span className="truncate">{item.name}</span>,
  }));
  const fallbackOptions = [
    {
      value: "",
      query: t("workspace_settings.settings.metric_sources.fallback_none"),
      content: t("workspace_settings.settings.metric_sources.fallback_none"),
    },
    ...serviceOptions,
  ];

  return (
    <ModalCore
      isOpen={isOpen}
      handleClose={handleClose}
      position={EModalPosition.TOP}
      width={EModalWidth.XXL}
      className="text-left"
    >
      <form
        onSubmit={handleSubmit}
        className="flex max-h-[calc(100dvh-5rem)] flex-col md:max-h-[calc(100dvh-10rem)]"
        noValidate
      >
        <div className="flex-1 overflow-y-auto p-5">
          <div>
            <h3 className="text-18 font-medium text-secondary">
              {t(
                source
                  ? "workspace_settings.settings.metric_sources.edit_title"
                  : "workspace_settings.settings.metric_sources.create_title"
              )}
            </h3>
            <p className="mt-1 text-13 text-tertiary">{t("workspace_settings.settings.metric_sources.description")}</p>
          </div>

          <div className="mt-5 space-y-5">
            <div className="space-y-1">
              <label htmlFor="metric-source-name" className="block text-11 font-medium text-secondary">
                {t("workspace_settings.settings.metric_sources.name")}
              </label>
              <UIKitInput
                id="metric-source-name"
                value={name}
                required
                maxLength={255}
                hasError={Boolean(nameError)}
                aria-invalid={Boolean(nameError)}
                aria-describedby={nameError ? "metric-source-name-error" : undefined}
                onChange={(event) => {
                  setName(event.target.value);
                  if (nameError) setNameError(null);
                }}
                placeholder="Prometheus production"
                className="w-full"
              />
              {nameError && (
                <p id="metric-source-name-error" className="text-11 text-danger-primary">
                  {nameError}
                </p>
              )}
            </div>

            <section className="space-y-2" aria-labelledby="metric-source-mapping-title">
              <h4 id="metric-source-mapping-title" className="text-11 font-medium text-secondary">
                {t("workspace_settings.settings.metric_sources.mapping_title")}
              </h4>
              <div className="hidden gap-2 text-11 text-tertiary sm:grid sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_2rem]">
                <span>{t("workspace_settings.settings.metric_sources.mapping_key")}</span>
                <span>{t("workspace_settings.settings.metric_sources.mapping_service")}</span>
                <span className="sr-only">{t("common.delete")}</span>
              </div>
              <div className="space-y-2">
                {mappings.map((mapping, index) => {
                  const selectedService = services.find((item) => item.id === mapping.serviceId);
                  return (
                    <div
                      key={mapping.id}
                      className="grid grid-cols-[minmax(0,1fr)_2rem] gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_2rem]"
                    >
                      <UIKitInput
                        value={mapping.key}
                        maxLength={200}
                        placeholder={t("workspace_settings.settings.metric_sources.mapping_key")}
                        aria-label={`${t("workspace_settings.settings.metric_sources.mapping_key")} ${index + 1}`}
                        onChange={(event) => updateMapping(index, { key: event.target.value })}
                        className="col-span-full w-full sm:col-span-1"
                      />
                      <CustomSearchSelect
                        input
                        value={mapping.serviceId}
                        onChange={(value: string) => updateMapping(index, { serviceId: value })}
                        options={serviceOptions}
                        label={
                          <span className={cn("truncate text-13", !selectedService && "text-secondary")}>
                            {selectedService?.name ?? t("workspace_settings.settings.metric_sources.mapping_service")}
                          </span>
                        }
                        buttonClassName={SELECT_BUTTON_CLASS}
                        className="w-full"
                      />
                      <IconButton
                        variant="ghost"
                        size="xl"
                        icon={CloseOutline}
                        aria-label={`${t("common.delete")} ${t(
                          "workspace_settings.settings.metric_sources.mapping_key"
                        )} ${index + 1}`}
                        onClick={() => removeMapping(index)}
                        className="justify-self-end sm:justify-self-auto"
                      />
                    </div>
                  );
                })}
              </div>
              <Button
                variant="secondary"
                size="lg"
                prependIcon={<AddOutline />}
                onClick={() => setMappings((current) => [...current, newMapping()])}
              >
                {t("workspace_settings.settings.metric_sources.add_mapping")}
              </Button>
            </section>

            <div className="grid grid-cols-1 gap-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
              <div className="space-y-1">
                <label className="block text-11 font-medium text-secondary">
                  {t("workspace_settings.settings.metric_sources.fallback")}
                </label>
                <CustomSearchSelect
                  input
                  value={fallback}
                  onChange={(value: string) => setFallback(value)}
                  options={fallbackOptions}
                  label={
                    <span className={cn("truncate text-13", !selectedFallback && "text-secondary")}>
                      {selectedFallback?.name ?? t("workspace_settings.settings.metric_sources.fallback_none")}
                    </span>
                  }
                  buttonClassName={SELECT_BUTTON_CLASS}
                  className="w-full"
                />
              </div>
              <div className="flex h-9 items-center gap-2">
                <Switch
                  size="sm"
                  checked={isActive}
                  onCheckedChange={setIsActive}
                  aria-label={t("workspace_settings.settings.metric_sources.active")}
                />
                <span className="text-13 text-secondary">{t("workspace_settings.settings.metric_sources.active")}</span>
              </div>
            </div>

            {error && (
              <p role="alert" className="rounded-md bg-danger-subtle px-3 py-2 text-12 text-danger-primary">
                {error}
              </p>
            )}
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
          <Button variant="secondary" size="lg" onClick={handleClose} disabled={saving}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" size="lg" type="submit" loading={saving}>
            {t("workspace_settings.settings.metric_sources.save")}
          </Button>
        </div>
      </form>
    </ModalCore>
  );
}
