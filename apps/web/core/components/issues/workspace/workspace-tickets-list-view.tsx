/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { STATE_GROUPS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { PriorityIcon, StateGroupIcon } from "@plane/propel/icons";
import { getFileURL, renderFormattedDate } from "@plane/utils";
// components
import { AvatarGroupOverflow } from "@/components/common/avatar-group-overflow";
import { ListItem } from "@/components/core/list";
import {
  WorkspaceAggregateList,
  type TWorkspaceAggregateGroupItem,
} from "@/components/workspace/aggregate-list/workspace-aggregate-list";
// helpers
import {
  appendTicketsPage,
  buildWorkspaceTicketsFilter,
  createTicketsPageState,
  deriveTicketGroup,
  formatIssueKey,
  groupWorkspaceTickets,
  normalizeTicketsEnvelope,
  type TWorkspaceTicketRow,
} from "@/lib/workspace-tickets";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useProject } from "@/hooks/store/use-project";
import { useProjectState } from "@/hooks/store/use-project-state";
import { useAppRouter } from "@/hooks/use-app-router";
// services
import { WorkspaceService } from "@/services/workspace.service";

const workspaceService = new WorkspaceService();

type TTicketRowProps = {
  ticket: TWorkspaceTicketRow;
  workspaceSlug: string;
};

const TicketRow = observer(function TicketRow({ ticket, workspaceSlug }: TTicketRowProps) {
  const parentRef = useRef(null);
  const { getProjectById } = useProject();
  const { getStateById } = useProjectState();
  const { getUserDetails } = useMember();

  const project = ticket.project_id ? getProjectById(ticket.project_id) : undefined;
  const state = getStateById(ticket.state_id);
  const stateGroup = deriveTicketGroup(ticket);
  const issueKey = project ? formatIssueKey(project.identifier, ticket.sequence_id) : `#${ticket.sequence_id}`;

  return (
    <ListItem
      title={`${issueKey} ${ticket.name}`}
      itemLink={`/${workspaceSlug}/browse/${issueKey}`}
      prependTitleElement={
        <Tooltip label={state?.name ?? STATE_GROUPS[stateGroup].label} disabled={!state}>
          <span className="flex items-center">
            <StateGroupIcon stateGroup={stateGroup} />
          </span>
        </Tooltip>
      }
      appendTitleElement={
        <div className="flex items-center gap-3 text-11 text-tertiary">
          {project && (
            <span className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5">{project.name}</span>
          )}
          <PriorityIcon priority={ticket.priority} size={14} />
          {ticket.assignee_ids.length > 0 && (
            <AvatarGroupOverflow size="xs">
              {ticket.assignee_ids.map((assigneeId) => {
                const member = getUserDetails(assigneeId);
                return (
                  <Avatar
                    key={assigneeId}
                    alt={member?.display_name}
                    fallback={member?.display_name?.[0]?.toUpperCase()}
                    src={getFileURL(member?.avatar_url ?? "")}
                  />
                );
              })}
            </AvatarGroupOverflow>
          )}
          <span className="hidden md:inline">{renderFormattedDate(ticket.updated_at)}</span>
        </div>
      }
      parentRef={parentRef}
    />
  );
});

type Props = {
  projectFilter?: string;
};

export const WorkspaceTicketsListView = observer(function WorkspaceTicketsListView({ projectFilter }: Props) {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const router = useAppRouter();
  const { workspaceProjectIds } = useProject();
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [pageState, setPageState] = useState(() => createTicketsPageState());
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingMore, setIsLoadingMore] = useState(false);

  const slug = workspaceSlug?.toString();
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
    const filters = buildWorkspaceTicketsFilter({ projectId: normalizedProjectFilter, search: debouncedSearch });
    const params: Record<string, string | number> = { per_page: 100, order_by: "-created_at" };
    if (filters) params.filters = filters;
    void workspaceService
      .getViewIssues(slug, params, { signal: controller.signal })
      .then((response) => setPageState(createTicketsPageState(normalizeTicketsEnvelope(response))))
      .catch(() => undefined)
      .finally(() => {
        if (!controller.signal.aborted) setIsLoading(false);
      });
    return () => controller.abort();
  }, [slug, normalizedProjectFilter, debouncedSearch]);

  const groups = useMemo(() => groupWorkspaceTickets(pageState.results), [pageState.results]);

  const renderedGroups: TWorkspaceAggregateGroupItem[] = groups.map((group) => ({
    key: group.key,
    label: STATE_GROUPS[group.key].label,
    count: group.tickets.length,
    items: (
      <>
        {group.tickets.map((ticket) => (
          <TicketRow key={ticket.id} ticket={ticket} workspaceSlug={slug ?? ""} />
        ))}
      </>
    ),
  }));

  const handleProjectFilterChange = (projectId: string) => {
    if (!slug) return;
    router.push(projectId ? `/${slug}/tickets?project=${projectId}` : `/${slug}/tickets`);
  };

  const handleLoadMore = () => {
    if (!slug || !pageState.hasMore || !pageState.nextCursor || isLoadingMore) return;
    setIsLoadingMore(true);
    const filters = buildWorkspaceTicketsFilter({ projectId: normalizedProjectFilter, search: debouncedSearch });
    const params: Record<string, string | number> = {
      per_page: 100,
      order_by: "-created_at",
      cursor: pageState.nextCursor,
    };
    if (filters) params.filters = filters;
    void workspaceService
      .getViewIssues(slug, params, {})
      .then((response) => setPageState((state) => appendTicketsPage(state, normalizeTicketsEnvelope(response))))
      .catch(() => undefined)
      .finally(() => setIsLoadingMore(false));
  };

  return (
    <WorkspaceAggregateList
      projectFilter={normalizedProjectFilter}
      onProjectFilterChange={handleProjectFilterChange}
      search={search}
      onSearchChange={setSearch}
      searchPlaceholder={t("service.catalog_search_placeholder")}
      groups={renderedGroups}
      totalCount={pageState.totalResults}
      isLoading={isLoading}
      emptyTitle={t("workspace_empty_state.tickets.title")}
      emptyDescription={t("workspace_empty_state.tickets.description")}
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
  );
});
