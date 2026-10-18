/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { useTranslation } from "@plane/i18n";
import { PriorityIcon } from "@plane/propel/icons";
import type { EInboxIssueStatus, TIssuePriorities } from "@plane/types";
import { getFileURL, renderFormattedDate } from "@plane/utils";
// components
import { AvatarGroupOverflow } from "@/components/common/avatar-group-overflow";
import { ListItem } from "@/components/core/list";
import { InboxStatusIcon } from "@/components/inbox/inbox-status-icon";
import {
  WorkspaceAggregateList,
  type TWorkspaceAggregateGroupItem,
} from "@/components/workspace/aggregate-list/workspace-aggregate-list";
// helpers
import {
  appendIntakePage,
  buildIntakeDetailUrl,
  buildWorkspaceIntakeParams,
  createIntakePageState,
  flatGroupIntakeResults,
  normalizeIntakeEnvelope,
  normalizeIntakeTab,
  type TWorkspaceIntakeRow,
  type TWorkspaceIntakeTab,
} from "@/lib/workspace-intake";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useProject } from "@/hooks/store/use-project";
import { useAppRouter } from "@/hooks/use-app-router";
// services
import { InboxIssueService } from "@/services/inbox/inbox-issue.service";

const intakeIssueService = new InboxIssueService();

const INTAKE_TAB_OPTIONS: { key: TWorkspaceIntakeTab; label: string }[] = [
  { key: "open", label: "Open" },
  { key: "closed", label: "Closed" },
];

type TIntakeRowProps = {
  row: TWorkspaceIntakeRow;
  workspaceSlug: string;
  tab: string;
};

const IntakeRow = observer(function IntakeRow({ row, workspaceSlug, tab }: TIntakeRowProps) {
  const parentRef = useRef(null);
  const { getProjectById } = useProject();
  const { getUserDetails } = useMember();

  const project = getProjectById(row.issue.project_id);
  const creator = row.issue.created_by ? getUserDetails(row.issue.created_by) : undefined;
  const issueKey = project ? `${project.identifier}-${row.issue.sequence_id}` : `#${row.issue.sequence_id}`;

  return (
    <ListItem
      title={`${issueKey} ${row.issue.name}`}
      itemLink={buildIntakeDetailUrl(workspaceSlug, project?.id ?? row.issue.project_id, tab, row.issue.id)}
      prependTitleElement={
        row.status !== -2 ? <InboxStatusIcon type={row.status as EInboxIssueStatus} size={14} /> : undefined
      }
      appendTitleElement={
        <div className="flex items-center gap-3 text-11 text-tertiary">
          {project && (
            <span className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5">{project.name}</span>
          )}
          {row.source && <span className="hidden rounded-sm bg-layer-2 px-1.5 py-0.5 md:inline">{row.source}</span>}
          <PriorityIcon priority={row.issue.priority as TIssuePriorities} size={14} />
          {creator && (
            <span className="hidden items-center gap-1.5 lg:flex">
              <AvatarGroupOverflow size="xs">
                <Avatar
                  alt={creator.display_name}
                  fallback={creator.display_name?.[0]?.toUpperCase()}
                  src={getFileURL(creator.avatar_url ?? "")}
                />
              </AvatarGroupOverflow>
              <span className="max-w-[100px] truncate">{creator.display_name}</span>
            </span>
          )}
          <span className="hidden md:inline">{renderFormattedDate(row.issue.created_at)}</span>
        </div>
      }
      parentRef={parentRef}
    />
  );
});

type Props = {
  typeFilter?: string;
  projectFilter?: string;
};

export const WorkspaceIntakeListView = observer(function WorkspaceIntakeListView({ typeFilter, projectFilter }: Props) {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const router = useAppRouter();
  const { workspaceProjectIds } = useProject();
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [pageState, setPageState] = useState(() => createIntakePageState());
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingMore, setIsLoadingMore] = useState(false);

  const slug = workspaceSlug?.toString();
  const normalizedTab = normalizeIntakeTab(typeFilter);
  const normalizedProjectFilter =
    projectFilter && (workspaceProjectIds ?? []).includes(projectFilter) ? projectFilter : undefined;

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedSearch(search), 300);
    return () => clearTimeout(timer);
  }, [search]);

  useEffect(() => {
    if (!slug) return;
    const controller = new AbortController();
    setIsLoading(true);
    const params = buildWorkspaceIntakeParams({
      typeFilter: normalizedTab,
      projectId: normalizedProjectFilter,
      search: debouncedSearch,
    });
    void intakeIssueService
      .listWorkspace(slug, params, { signal: controller.signal })
      .then((response) => setPageState(createIntakePageState(normalizeIntakeEnvelope(response))))
      .catch(() => undefined)
      .finally(() => {
        if (!controller.signal.aborted) setIsLoading(false);
      });
    return () => controller.abort();
  }, [slug, normalizedTab, normalizedProjectFilter, debouncedSearch]);

  const groups: TWorkspaceAggregateGroupItem[] = flatGroupIntakeResults(pageState.results).map((group) => ({
    key: group.key,
    label: group.label,
    count: group.rows.length,
    items: (
      <>
        {group.rows.map((row) => (
          <IntakeRow key={row.id} row={row} workspaceSlug={slug ?? ""} tab={normalizedTab} />
        ))}
      </>
    ),
  }));

  const buildTabHref = (nextTab: TWorkspaceIntakeTab) => {
    if (!slug) return "#";
    const params = new URLSearchParams();
    params.set("currentTab", nextTab);
    if (normalizedProjectFilter) params.set("project", normalizedProjectFilter);
    return `/${slug}/intake?${params.toString()}`;
  };

  const handleProjectFilterChange = (projectId: string) => {
    if (!slug) return;
    const params = new URLSearchParams();
    params.set("currentTab", normalizedTab);
    if (projectId) params.set("project", projectId);
    router.push(`/${slug}/intake?${params.toString()}`);
  };

  const handleLoadMore = () => {
    if (!slug || !pageState.hasMore || !pageState.nextCursor || isLoadingMore) return;
    setIsLoadingMore(true);
    const params = {
      ...buildWorkspaceIntakeParams({
        typeFilter: normalizedTab,
        projectId: normalizedProjectFilter,
        search: debouncedSearch,
      }),
      cursor: pageState.nextCursor,
    };
    void intakeIssueService
      .listWorkspace(slug, params, {})
      .then((response) => setPageState((state) => appendIntakePage(state, normalizeIntakeEnvelope(response))))
      .catch(() => undefined)
      .finally(() => setIsLoadingMore(false));
  };

  return (
    <div className="relative flex min-h-0 w-full flex-1 flex-col">
      <div className="flex items-center gap-1 border-b border-subtle px-3 py-1.5">
        {INTAKE_TAB_OPTIONS.map((tab) => (
          <button
            key={tab.key}
            type="button"
            onClick={() => router.push(buildTabHref(tab.key))}
            className={
              tab.key === normalizedTab
                ? "rounded-sm bg-layer-3 px-2.5 py-1 text-12 font-medium text-primary"
                : "rounded-sm px-2.5 py-1 text-12 text-secondary hover:bg-layer-transparent-hover"
            }
          >
            {tab.label}
          </button>
        ))}
      </div>
      <WorkspaceAggregateList
        flat
        projectFilter={normalizedProjectFilter}
        onProjectFilterChange={handleProjectFilterChange}
        search={search}
        onSearchChange={setSearch}
        searchPlaceholder={t("service.catalog_search_placeholder")}
        groups={groups}
        totalCount={pageState.totalResults}
        isLoading={isLoading}
        emptyTitle={t("workspace_empty_state.intake.title")}
        emptyDescription={t("workspace_empty_state.intake.description")}
        footer={
          pageState.hasMore ? (
            <div className="flex items-center justify-center border-b border-subtle py-3">
              <button
                type="button"
                onClick={handleLoadMore}
                disabled={isLoadingMore}
                className="rounded-sm border border-subtle px-3 py-1 text-12 text-secondary transition-colors hover:bg-layer-transparent-hover disabled:cursor-not-allowed disabled:opacity-50"
              >
                {isLoadingMore ? t("common.loading") : t("common.load_more")}
              </button>
            </div>
          ) : undefined
        }
      />
    </div>
  );
});
