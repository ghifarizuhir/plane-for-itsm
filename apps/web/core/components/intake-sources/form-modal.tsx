/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useState } from "react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { TIntakeSource } from "@plane/types";
import { EModalPosition, EModalWidth, Input, ModalCore } from "@plane/ui";
// hooks
import { useIntakeSource } from "@/hooks/store/use-intake-source";
import { useService } from "@/hooks/store/use-service";
import { useWorkItemType } from "@/hooks/store/use-work-item-type";
// helpers
import {
  configFromForm,
  INTAKE_PRIORITIES,
  nextRowId,
  serviceRowsFromConfig,
  severityRowsFromConfig,
  validateIntakeSourceForm,
  type TIntakeSourceForm,
} from "@/store/intake-source.helpers";

type Props = {
  workspaceSlug: string;
  projectId: string;
  source: TIntakeSource | null;
  isOpen: boolean;
  onClose: () => void;
};

export const IntakeSourceFormModal = observer(function IntakeSourceFormModal(props: Props) {
  const { workspaceSlug, projectId, source, isOpen, onClose } = props;
  // translation
  const { t } = useTranslation();
  // store hooks
  const { createSource, updateSource } = useIntakeSource();
  const { getProjectServiceIds, getServiceById } = useService();
  const { workItemTypes } = useWorkItemType();
  // derived values
  const projectTypes = (workItemTypes ?? []).filter(
    (type) => !type.is_epic && type.is_active && type.project_ids.includes(projectId)
  );
  const serviceIds = getProjectServiceIds(projectId) ?? [];

  const severityDefaults = severityRowsFromConfig(source?.config);
  const [form, setForm] = useState<TIntakeSourceForm>({
    name: source?.name ?? "",
    typeId: source?.type_id ?? "",
    autoAccept: source?.auto_accept ?? false,
    serviceLabelKey: source?.config?.service_label_key ?? "service",
    serviceRows: serviceRowsFromConfig(source?.config),
    fallbackServiceId: source?.config?.fallback_service_id ?? null,
    severityLabelKey: source?.config?.severity_label_key ?? "severity",
    severityRows:
      severityDefaults.length > 0
        ? severityDefaults
        : [
            { id: nextRowId(), labelValue: "critical", priority: "urgent" },
            { id: nextRowId(), labelValue: "warning", priority: "high" },
            { id: nextRowId(), labelValue: "info", priority: "low" },
          ],
    defaultPriority: source?.config?.default_priority ?? "none",
  });
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const handleSubmit = async () => {
    const validation = validateIntakeSourceForm(form, serviceIds);
    if (validation) {
      setError(validation);
      return;
    }
    setSaving(true);
    try {
      const payload = {
        name: form.name.trim(),
        type_id: form.typeId,
        auto_accept: form.autoAccept,
        config: configFromForm(form),
      };
      if (source) await updateSource(workspaceSlug, projectId, source.id, payload);
      else await createSource(workspaceSlug, projectId, payload);
      onClose();
    } catch {
      setError(t("project_settings.intake_sources.save_error"));
    } finally {
      setSaving(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.XXL}>
      <div className="flex max-h-[85vh] flex-col gap-4 overflow-y-auto p-5">
        <h3 className="text-h4-medium">
          {source ? t("project_settings.intake_sources.edit_source") : t("project_settings.intake_sources.add_source")}
        </h3>
        {error && <p className="text-13 text-danger-primary">{error}</p>}

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.name")}</span>
            <Input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} />
          </label>
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.type")}</span>
            <select
              className="rounded-md border border-subtle bg-layer-1 px-2 py-2 text-13"
              value={form.typeId}
              onChange={(e) => setForm({ ...form, typeId: e.target.value })}
            >
              <option value="">{t("project_settings.intake_sources.form.select_type")}</option>
              {projectTypes.map((type) => (
                <option key={type.id} value={type.id}>
                  {type.name}
                </option>
              ))}
            </select>
          </label>
          <label className="flex items-center gap-2 text-13">
            <input
              type="checkbox"
              checked={form.autoAccept}
              onChange={(e) => setForm({ ...form, autoAccept: e.target.checked })}
            />
            <span>{t("project_settings.intake_sources.form.auto_accept")}</span>
          </label>
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.default_priority")}</span>
            <select
              className="rounded-md border border-subtle bg-layer-1 px-2 py-2 text-13"
              value={form.defaultPriority}
              onChange={(e) =>
                setForm({ ...form, defaultPriority: e.target.value as TIntakeSourceForm["defaultPriority"] })
              }
            >
              {INTAKE_PRIORITIES.map((priority) => (
                <option key={priority} value={priority}>
                  {priority}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="flex flex-col gap-2 rounded-md border border-subtle p-3">
          <p className="text-body-sm-medium">{t("project_settings.intake_sources.form.service_mapping")}</p>
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.service_label_key")}</span>
            <Input
              value={form.serviceLabelKey}
              onChange={(e) => setForm({ ...form, serviceLabelKey: e.target.value })}
            />
          </label>
          {form.serviceRows.map((row) => (
            <div key={row.id} className="flex items-center gap-2">
              <Input
                placeholder={t("project_settings.intake_sources.form.label_value")}
                value={row.labelValue}
                onChange={(e) => {
                  const rows = form.serviceRows.map((existing) =>
                    existing.id === row.id ? { ...existing, labelValue: e.target.value } : existing
                  );
                  setForm({ ...form, serviceRows: rows });
                }}
              />
              <select
                className="w-full rounded-md border border-subtle bg-layer-1 px-2 py-2 text-13"
                value={row.serviceId}
                onChange={(e) => {
                  const rows = form.serviceRows.map((existing) =>
                    existing.id === row.id ? { ...existing, serviceId: e.target.value } : existing
                  );
                  setForm({ ...form, serviceRows: rows });
                }}
              >
                <option value="">{t("project_settings.intake_sources.form.select_service")}</option>
                {serviceIds.map((serviceId) => (
                  <option key={serviceId} value={serviceId}>
                    {getServiceById(serviceId)?.name ?? serviceId}
                  </option>
                ))}
              </select>
              <Button
                variant="secondary"
                size="sm"
                onClick={() =>
                  setForm({ ...form, serviceRows: form.serviceRows.filter((existing) => existing.id !== row.id) })
                }
              >
                {t("common.remove")}
              </Button>
            </div>
          ))}
          <div>
            <Button
              variant="secondary"
              size="sm"
              onClick={() =>
                setForm({
                  ...form,
                  serviceRows: [...form.serviceRows, { id: nextRowId(), labelValue: "", serviceId: "" }],
                })
              }
            >
              {t("project_settings.intake_sources.form.add_row")}
            </Button>
          </div>
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.fallback_service")}</span>
            <select
              className="rounded-md border border-subtle bg-layer-1 px-2 py-2 text-13"
              value={form.fallbackServiceId ?? ""}
              onChange={(e) => setForm({ ...form, fallbackServiceId: e.target.value || null })}
            >
              <option value="">{t("project_settings.intake_sources.form.no_fallback")}</option>
              {serviceIds.map((serviceId) => (
                <option key={serviceId} value={serviceId}>
                  {getServiceById(serviceId)?.name ?? serviceId}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="flex flex-col gap-2 rounded-md border border-subtle p-3">
          <p className="text-body-sm-medium">{t("project_settings.intake_sources.form.severity_mapping")}</p>
          <label className="flex flex-col gap-1 text-13">
            <span className="text-tertiary">{t("project_settings.intake_sources.form.severity_label_key")}</span>
            <Input
              value={form.severityLabelKey}
              onChange={(e) => setForm({ ...form, severityLabelKey: e.target.value })}
            />
          </label>
          {form.severityRows.map((row) => (
            <div key={row.id} className="flex items-center gap-2">
              <Input
                placeholder={t("project_settings.intake_sources.form.label_value")}
                value={row.labelValue}
                onChange={(e) => {
                  const rows = form.severityRows.map((existing) =>
                    existing.id === row.id ? { ...existing, labelValue: e.target.value } : existing
                  );
                  setForm({ ...form, severityRows: rows });
                }}
              />
              <select
                className="w-full rounded-md border border-subtle bg-layer-1 px-2 py-2 text-13"
                value={row.priority}
                onChange={(e) => {
                  const rows = form.severityRows.map((existing) =>
                    existing.id === row.id
                      ? { ...existing, priority: e.target.value as TIntakeSourceForm["defaultPriority"] }
                      : existing
                  );
                  setForm({ ...form, severityRows: rows });
                }}
              >
                {INTAKE_PRIORITIES.map((priority) => (
                  <option key={priority} value={priority}>
                    {priority}
                  </option>
                ))}
              </select>
              <Button
                variant="secondary"
                size="sm"
                onClick={() =>
                  setForm({ ...form, severityRows: form.severityRows.filter((existing) => existing.id !== row.id) })
                }
              >
                {t("common.remove")}
              </Button>
            </div>
          ))}
          <div>
            <Button
              variant="secondary"
              size="sm"
              onClick={() =>
                setForm({
                  ...form,
                  severityRows: [...form.severityRows, { id: nextRowId(), labelValue: "", priority: "none" }],
                })
              }
            >
              {t("project_settings.intake_sources.form.add_row")}
            </Button>
          </div>
        </div>

        <div className="flex justify-end gap-2">
          <Button variant="secondary" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" size="sm" loading={saving} onClick={() => void handleSubmit()}>
            {source
              ? t("project_settings.intake_sources.update_source")
              : t("project_settings.intake_sources.create_source")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
