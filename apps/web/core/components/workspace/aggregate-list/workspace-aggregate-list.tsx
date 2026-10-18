/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ReactNode } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { CustomSelect } from "@plane/ui";
// hooks
import { useProject } from "@/hooks/store/use-project";

export type TWorkspaceAggregateGroupItem = {
  key: string;
  label: string;
  count: number;
  items: ReactNode;
};

type Props = {
  projectFilter?: string;
  onProjectFilterChange: (projectId: string) => void;
  search: string;
  onSearchChange: (value: string) => void;
  searchPlaceholder: string;
  groups: TWorkspaceAggregateGroupItem[];
  totalCount: number;
  isLoading?: boolean;
  emptyTitle: string;
  emptyDescription: string;
  footer?: ReactNode;
  flat?: boolean;
};

export const WorkspaceAggregateList = observer(function WorkspaceAggregateList({
  projectFilter,
  onProjectFilterChange,
  search,
  onSearchChange,
  searchPlaceholder,
  groups,
  totalCount,
  isLoading = false,
  emptyTitle,
  emptyDescription,
  footer,
  flat = false,
}: Props) {
  const { t } = useTranslation();
  const { workspaceProjectIds, getProjectById } = useProject();

  return (
    <div className="relative flex min-h-0 w-full flex-1 flex-col">
      <div className="flex items-center justify-between gap-2 border-b border-subtle px-3 py-2">
        <div className="flex items-center gap-2">
          <CustomSelect
            value={projectFilter ?? ""}
            onChange={onProjectFilterChange}
            label={
              <span className="flex items-center gap-2 py-0.5 text-12">
                {projectFilter ? getProjectById(projectFilter)?.name : t("service.catalog_all_projects")}
              </span>
            }
            noChevron
          >
            <CustomSelect.Option value="">{t("service.catalog_all_projects")}</CustomSelect.Option>
            {(workspaceProjectIds ?? []).map((id) => {
              const project = getProjectById(id);
              return (
                <CustomSelect.Option key={id} value={id}>
                  <span className="truncate">{project?.name}</span>
                </CustomSelect.Option>
              );
            })}
          </CustomSelect>
          <input
            value={search}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder={searchPlaceholder}
            className="h-7 rounded-sm border border-subtle bg-layer-1 px-2 text-12 text-primary outline-none"
          />
          <span className="text-12 text-tertiary">{totalCount}</span>
        </div>
      </div>
      {isLoading ? (
        <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>
      ) : totalCount === 0 ? (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{emptyTitle}</p>
          <p className="text-xs text-secondary">{emptyDescription}</p>
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {flat ? (
            <>{groups.flatMap((group) => group.items)}</>
          ) : (
            groups.map((group) => (
              <div key={group.key}>
                <div className="flex items-center gap-2 border-b border-subtle bg-layer-1 px-4 py-2">
                  <span className="text-12 font-medium text-secondary">{group.label}</span>
                  <span className="text-11 text-tertiary">{group.count}</span>
                </div>
                {group.items}
              </div>
            ))
          )}
          {footer}
        </div>
      )}
    </div>
  );
});
