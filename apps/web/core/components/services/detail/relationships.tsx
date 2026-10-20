/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// icons
import { LockOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService, IServiceDependency, TServiceDependencyType } from "@plane/types";
import { ControlLink, CustomSelect } from "@plane/ui";
import { cn } from "@plane/utils";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// helpers
import { dependencyExists, isLiteService, SERVICE_DEPENDENCY_TYPES } from "@/services/service.helpers";

type Props = {
  serviceId: string;
  layout?: "page" | "pane";
};

type TDirection = "out" | "in";

type TGroup = {
  key: string;
  type: TServiceDependencyType;
  direction: TDirection;
  labelKey: string;
};

const GROUPS: TGroup[] = [
  { key: "depends_on_out", type: "depends_on", direction: "out", labelKey: "service.detail.depends_on" },
  { key: "depends_on_in", type: "depends_on", direction: "in", labelKey: "service.relationships.required_by" },
  { key: "uses_out", type: "uses", direction: "out", labelKey: "service.relationships.uses" },
  { key: "uses_in", type: "uses", direction: "in", labelKey: "service.relationships.used_by" },
  { key: "runs_on_out", type: "runs_on", direction: "out", labelKey: "service.relationships.runs_on" },
  { key: "runs_on_in", type: "runs_on", direction: "in", labelKey: "service.relationships.hosts" },
  { key: "connects_to_out", type: "connects_to", direction: "out", labelKey: "service.relationships.connects_to" },
  { key: "connects_to_in", type: "connects_to", direction: "in", labelKey: "service.relationships.connected_from" },
  {
    key: "replicates_to_out",
    type: "replicates_to",
    direction: "out",
    labelKey: "service.relationships.replicates_to",
  },
  {
    key: "replicates_to_in",
    type: "replicates_to",
    direction: "in",
    labelKey: "service.relationships.replicated_from",
  },
  { key: "other_out", type: "other", direction: "out", labelKey: "service.relationships.related_to" },
  { key: "other_in", type: "other", direction: "in", labelKey: "service.relationships.related_to" },
];

const TYPE_LABEL_KEYS: Record<TServiceDependencyType, string> = {
  depends_on: "service.detail.depends_on",
  uses: "service.relationships.uses",
  runs_on: "service.relationships.runs_on",
  connects_to: "service.relationships.connects_to",
  replicates_to: "service.relationships.replicates_to",
  other: "service.relationships.related_to",
};

export const ServiceRelationships = observer(function ServiceRelationships(props: Props) {
  const { serviceId, layout = "page" } = props;
  // derived values
  const isPane = layout === "pane";
  // states
  const [selectedType, setSelectedType] = useState<TServiceDependencyType>("depends_on");
  const [selectedId, setSelectedId] = useState("");
  const [isAdding, setIsAdding] = useState(false);
  const [updatingId, setUpdatingId] = useState<string | null>(null);
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    getServiceById,
    getDependenciesByService,
    getProjectServiceIds,
    getWorkspaceServiceIds,
    addDependency,
    updateDependency,
    removeDependency,
  } = useService();
  const { getProjectById } = useProject();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString();
  const workspaceId = currentWorkspace?.id;
  const dependencyProjectId = projectId?.toString() ?? null;
  const dependencies = getDependenciesByService(serviceId);
  const selectedService = selectedId ? getServiceById(selectedId) : null;
  const candidateIds = projectId ? (getProjectServiceIds(projectId.toString()) ?? []) : getWorkspaceServiceIds();
  const candidates = candidateIds
    .map((id) => getServiceById(id))
    .filter((candidate): candidate is IService => Boolean(candidate) && candidate!.id !== serviceId)
    .filter((candidate) => !dependencyExists(dependencies, serviceId, candidate.id, selectedType));
  const canAdd = Boolean(selectedId) && !dependencyExists(dependencies, serviceId, selectedId, selectedType);

  const handleAdd = async () => {
    if (!slug || !workspaceId || !selectedId || !canAdd) return;
    setIsAdding(true);
    try {
      await addDependency(slug, workspaceId, dependencyProjectId, serviceId, selectedId, selectedType);
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

  const handleTypeChange = async (dependency: IServiceDependency, dependencyType: TServiceDependencyType) => {
    if (!slug || !workspaceId || (dependency.dependency_type ?? "depends_on") === dependencyType) return;
    setUpdatingId(dependency.id);
    try {
      await updateDependency(slug, workspaceId, dependencyProjectId, dependency.id, dependencyType);
    } catch (error) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: error instanceof Error ? error.message : t("service.relationships.update_error"),
      });
    } finally {
      setUpdatingId(null);
    }
  };

  const handleRemove = async (dependencyId: string) => {
    if (!slug || !workspaceId) return;
    try {
      await removeDependency(slug, workspaceId, dependencyProjectId, dependencyId);
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: t("service.detail.delete_dependency_error"),
      });
    }
  };

  const renderRow = (dependency: IServiceDependency, direction: TDirection) => {
    const otherId = direction === "out" ? dependency.to_service_id : dependency.from_service_id;
    const embedded = dependency.from_service_id === otherId ? dependency.from_service : dependency.to_service;
    const other = getServiceById(otherId);
    const name = embedded?.name ?? other?.name ?? otherId;
    const lite = Boolean(embedded && isLiteService(embedded));
    const status = embedded?.status ?? other?.status;
    const criticality = embedded?.criticality ?? other?.criticality;
    const ownerProject = other?.owner_project_id ? getProjectById(other.owner_project_id) : undefined;
    const dependencyType = dependency.dependency_type ?? "depends_on";
    const href = slug ? `/${slug}/services/${otherId}` : "#";

    return (
      <div
        key={dependency.id}
        className={cn(
          "flex gap-2 rounded-sm border border-subtle px-2 py-1.5",
          isPane ? "flex-col" : "items-center justify-between"
        )}
      >
        <span className={cn("flex min-w-0 items-center gap-1.5", isPane && "flex-wrap")} title={name}>
          {lite && <LockOutline className="h-3 w-3 shrink-0 text-tertiary" />}
          <ControlLink
            href={href}
            onClick={() => router.push(href)}
            className={cn("truncate text-12", lite ? "text-tertiary" : "text-primary")}
          >
            {name}
          </ControlLink>
          {lite && <span className="shrink-0 text-10 text-tertiary">{t("service.lite.restricted")}</span>}
          {!lite && status && (
            <span className="shrink-0 rounded-xs border border-subtle px-1 text-10 text-tertiary uppercase">
              {t(`service.status_values.${status}`)}
            </span>
          )}
          {!lite && criticality && (
            <span className="shrink-0 rounded-xs border border-subtle px-1 text-10 text-tertiary uppercase">
              {t(`service.criticality_values.${criticality}`)}
            </span>
          )}
          {!lite && ownerProject && (
            <span
              className="shrink-0 rounded-xs border border-subtle px-1 text-10 font-medium tracking-wide text-tertiary"
              title={ownerProject.name}
            >
              {ownerProject.identifier}
            </span>
          )}
        </span>
        {!lite && (
          <span className={cn("flex shrink-0 items-center gap-2", isPane && "justify-between")}>
            <CustomSelect
              value={dependencyType}
              onChange={(value: TServiceDependencyType) => void handleTypeChange(dependency, value)}
              disabled={updatingId === dependency.id}
              noChevron
              label={<span className="text-11 text-tertiary">{t(TYPE_LABEL_KEYS[dependencyType])}</span>}
            >
              {SERVICE_DEPENDENCY_TYPES.map((type) => (
                <CustomSelect.Option key={type} value={type}>
                  {t(TYPE_LABEL_KEYS[type])}
                </CustomSelect.Option>
              ))}
            </CustomSelect>
            <button
              type="button"
              onClick={() => void handleRemove(dependency.id)}
              className="text-11 text-tertiary hover:text-primary"
            >
              {t("remove")}
            </button>
          </span>
        )}
      </div>
    );
  };

  return (
    <div className="flex flex-col gap-3">
      {!isPane && <h4 className="text-13 font-medium text-secondary">{t("service.relationships.title")}</h4>}
      <div className={cn("flex items-center gap-2", isPane && "flex-col items-stretch")}>
        <CustomSelect
          value={selectedType}
          onChange={(value: TServiceDependencyType) => setSelectedType(value)}
          className={cn("h-7.5 shrink-0", isPane ? "w-full" : "w-[160px]")}
          label={<span className="truncate text-12">{t(TYPE_LABEL_KEYS[selectedType])}</span>}
        >
          {SERVICE_DEPENDENCY_TYPES.map((type) => (
            <CustomSelect.Option key={type} value={type}>
              {t(TYPE_LABEL_KEYS[type])}
            </CustomSelect.Option>
          ))}
        </CustomSelect>
        <CustomSelect
          value={selectedId}
          onChange={(value: string) => setSelectedId(value)}
          className={cn("h-7.5 min-w-0", isPane ? "w-full" : "grow basis-0")}
          label={
            <span className="flex items-center gap-2 py-0.5 text-12">
              {selectedService ? (
                <span className="truncate">{selectedService.name}</span>
              ) : (
                <span className="text-secondary">{t("service.detail.select_service")}</span>
              )}
            </span>
          }
        >
          {candidates.map((candidate) => (
            <CustomSelect.Option key={candidate.id} value={candidate.id}>
              <span className="truncate">{candidate.name}</span>
            </CustomSelect.Option>
          ))}
        </CustomSelect>
        <Button
          variant="secondary"
          size="sm"
          onClick={handleAdd}
          disabled={!canAdd}
          loading={isAdding}
          className={cn(isPane && "w-full")}
        >
          {t("add")}
        </Button>
      </div>
      {dependencies.length === 0 ? (
        <p className="text-12 text-tertiary">{t("service.relationships.empty")}</p>
      ) : (
        <div className="flex flex-col gap-4">
          {GROUPS.map((group) => {
            const rows = dependencies.filter((dependency) => {
              const type = dependency.dependency_type ?? "depends_on";
              return (
                type === group.type &&
                (group.direction === "out"
                  ? dependency.from_service_id === serviceId
                  : dependency.to_service_id === serviceId)
              );
            });
            if (rows.length === 0) return null;
            return (
              <div key={group.key} className="flex flex-col gap-1.5">
                <p className="text-12 font-medium text-tertiary">{t(group.labelKey)}</p>
                {rows.map((dependency) => renderRow(dependency, group.direction))}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
});
