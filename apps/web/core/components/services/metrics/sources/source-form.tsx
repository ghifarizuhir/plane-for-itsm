/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IMetricSource, IService } from "@plane/types";
import { CustomSelect, EModalPosition, EModalWidth, Input as UIKitInput, ModalCore } from "@plane/ui";
import { ServiceMetricsService } from "@/services/service-metrics.service";

const service = new ServiceMetricsService();

type Mapping = { id: string; key: string; serviceId: string };

const newMapping = (): Mapping => ({
  id: typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`,
  key: "",
  serviceId: "",
});

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
    setError(null);
  }, [isOpen, source]);

  const submit = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError(t("workspace_settings.settings.metric_sources.name_required"));
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

  return (
    <ModalCore isOpen={isOpen} position={EModalPosition.TOP} width={EModalWidth.XXL} className="p-4">
      <h3 className="text-h4-medium text-primary">
        {t(
          source
            ? "workspace_settings.settings.metric_sources.edit_title"
            : "workspace_settings.settings.metric_sources.create_title"
        )}
      </h3>
      <div className="mt-4 space-y-4">
        <div>
          <label className="mb-1 block text-11 text-tertiary">
            {t("workspace_settings.settings.metric_sources.name")}
          </label>
          <UIKitInput
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Prometheus production"
          />
        </div>

        <div>
          <p className="mb-2 text-11 text-tertiary">{t("workspace_settings.settings.metric_sources.mapping_title")}</p>
          <div className="space-y-2">
            {mappings.map((mapping, index) => (
              <div key={mapping.id} className="flex gap-2">
                <UIKitInput
                  value={mapping.key}
                  placeholder={t("workspace_settings.settings.metric_sources.mapping_key")}
                  onChange={(event) => {
                    const next = [...mappings];
                    next[index] = { ...next[index], key: event.target.value };
                    setMappings(next);
                  }}
                />
                <CustomSelect
                  value={mapping.serviceId}
                  label={
                    services.find((item) => item.id === mapping.serviceId)?.name ??
                    t("workspace_settings.settings.metric_sources.mapping_service")
                  }
                  onChange={(value: string) => {
                    const next = [...mappings];
                    next[index] = { ...next[index], serviceId: value };
                    setMappings(next);
                  }}
                  noChevron
                  className="w-full"
                >
                  {services.map((item) => (
                    <CustomSelect.Option key={item.id} value={item.id}>
                      {item.name}
                    </CustomSelect.Option>
                  ))}
                </CustomSelect>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => setMappings(mappings.filter((_, i) => i !== index))}
                >
                  {t("common.delete")}
                </Button>
              </div>
            ))}
          </div>
          <Button
            variant="secondary"
            size="sm"
            className="mt-2"
            onClick={() => setMappings([...mappings, newMapping()])}
          >
            {t("workspace_settings.settings.metric_sources.add_mapping")}
          </Button>
        </div>

        <div>
          <label className="mb-1 block text-11 text-tertiary">
            {t("workspace_settings.settings.metric_sources.fallback")}
          </label>
          <CustomSelect
            value={fallback}
            label={
              services.find((item) => item.id === fallback)?.name ??
              t("workspace_settings.settings.metric_sources.fallback_none")
            }
            onChange={(value: string) => setFallback(value)}
            noChevron
            className="w-full"
          >
            <CustomSelect.Option value="">
              {t("workspace_settings.settings.metric_sources.fallback_none")}
            </CustomSelect.Option>
            {services.map((item) => (
              <CustomSelect.Option key={item.id} value={item.id}>
                {item.name}
              </CustomSelect.Option>
            ))}
          </CustomSelect>
        </div>

        <label className="flex items-center gap-2 text-13 text-secondary">
          <input type="checkbox" checked={isActive} onChange={(event) => setIsActive(event.target.checked)} />
          {t("workspace_settings.settings.metric_sources.active")}
        </label>

        {error && <p className="text-12 text-danger-primary">{error}</p>}

        <div className="flex justify-end gap-2">
          <Button variant="secondary" size="sm" onClick={onClose} disabled={saving}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" size="sm" onClick={submit} loading={saving}>
            {t("workspace_settings.settings.metric_sources.save")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
}
