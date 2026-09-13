/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService } from "@plane/types";
// ui
import { CustomSelect } from "@plane/ui";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  serviceId: string;
};

export const ServiceDependencies = observer(function ServiceDependencies(props: Props) {
  const { serviceId } = props;
  // states
  const [selectedId, setSelectedId] = useState<string>("");
  const [isAdding, setIsAdding] = useState(false);
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, getDependenciesByProject, getProjectServiceIds, addDependency, removeDependency } =
    useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString();
  const pid = projectId?.toString() ?? service.project_id;
  const workspaceId = currentWorkspace?.id;
  const dependencies = getDependenciesByProject(pid);
  const outgoing = dependencies.filter((d) => d.from_service_id === serviceId);
  const incoming = dependencies.filter((d) => d.to_service_id === serviceId);
  const projectServiceIds = getProjectServiceIds(pid) ?? [];
  const candidates = projectServiceIds
    .map((id) => getServiceById(id))
    .filter((s): s is IService => {
      if (!s) return false;
      return s.id !== serviceId && !outgoing.some((d) => d.to_service_id === s.id);
    });
  const selectedService = selectedId ? getServiceById(selectedId) : null;

  const handleAdd = async () => {
    if (!slug || !workspaceId || !pid || !selectedId) return;
    setIsAdding(true);
    try {
      await addDependency(slug, workspaceId, pid, serviceId, selectedId);
      setSelectedId("");
    } catch (error) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: error instanceof Error ? error.message : t("service.detail.create_dependency_error"),
      });
    } finally {
      setIsAdding(false);
    }
  };

  const handleRemove = async (dependencyId: string) => {
    if (!slug || !workspaceId || !pid) return;
    try {
      await removeDependency(slug, workspaceId, pid, dependencyId);
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: t("service.detail.delete_dependency_error"),
      });
    }
  };

  const renderEdgeRow = (dependencyId: string, otherServiceId: string) => {
    const other = getServiceById(otherServiceId);
    return (
      <div
        key={dependencyId}
        className="flex items-center justify-between gap-2 rounded-md border border-subtle px-3 py-2"
      >
        <span className="truncate text-13 text-primary" title={other?.name ?? otherServiceId}>
          {other?.name ?? otherServiceId}
        </span>
        <button
          type="button"
          onClick={() => handleRemove(dependencyId)}
          className="shrink-0 text-12 text-tertiary hover:text-primary"
        >
          {t("remove")}
        </button>
      </div>
    );
  };

  return (
    <div className="flex max-w-3xl flex-col gap-4">
      <div className="flex items-center gap-2">
        <CustomSelect
          value={selectedId}
          onChange={(val: string) => setSelectedId(val)}
          label={
            <span className="flex items-center gap-2 py-0.5 text-12">
              {selectedService ? (
                selectedService.name
              ) : (
                <span className="text-secondary">{t("service.detail.select_service")}</span>
              )}
            </span>
          }
        >
          {candidates.map((s) => (
            <CustomSelect.Option key={s.id} value={s.id}>
              <span className="truncate">{s.name}</span>
            </CustomSelect.Option>
          ))}
        </CustomSelect>
        <Button variant="primary" size="sm" onClick={handleAdd} disabled={!selectedId} loading={isAdding}>
          {t("add")}
        </Button>
      </div>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <div className="flex flex-col gap-1.5">
          <p className="text-12 font-medium text-tertiary">{t("service.detail.depends_on")}</p>
          {outgoing.length === 0 ? (
            <p className="text-12 text-tertiary">{t("service.detail.no_dependencies")}</p>
          ) : (
            outgoing.map((d) => renderEdgeRow(d.id, d.to_service_id))
          )}
        </div>
        <div className="flex flex-col gap-1.5">
          <p className="text-12 font-medium text-tertiary">{t("service.detail.depended_on_by")}</p>
          {incoming.length === 0 ? (
            <p className="text-12 text-tertiary">{t("service.detail.no_dependents")}</p>
          ) : (
            incoming.map((d) => renderEdgeRow(d.id, d.from_service_id))
          )}
        </div>
      </div>
    </div>
  );
});
