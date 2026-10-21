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
import { useTranslation } from "@plane/i18n";
import { PagesOutline } from "@makeplane/propel/icons";
import { Logo } from "@plane/propel/emoji-icon-picker";
import { getFileURL, getPageName, renderFormattedDate } from "@plane/utils";
// components
import { AvatarGroupOverflow } from "@/components/common/avatar-group-overflow";
import { ListItem } from "@/components/core/list";
import {
  WorkspaceAggregateList,
  type TWorkspaceAggregateGroupItem,
} from "@/components/workspace/aggregate-list/workspace-aggregate-list";
// helpers
import {
  appendKbPage,
  buildWorkspaceKbParams,
  createKbPageState,
  groupWorkspaceKbPages,
  normalizeKbResponse,
  normalizeWorkspaceKbType,
  type TWorkspaceKbPage,
  type TWorkspaceKbTab,
} from "@/lib/workspace-kb";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useProject } from "@/hooks/store/use-project";
import { useAppRouter } from "@/hooks/use-app-router";
// services
import { ProjectPageService } from "@/services/page/project-page.service";

const pageService = new ProjectPageService();

const KB_TAB_OPTIONS: { key: TWorkspaceKbTab; label: string }[] = [
  { key: "all", label: "All" },
  { key: "public", label: "Public" },
  { key: "private", label: "Private" },
  { key: "archived", label: "Archived" },
];

type TKbRowProps = {
  page: TWorkspaceKbPage;
  projectId: string;
  workspaceSlug: string;
};

const KbRow = observer(function KbRow({ page, projectId, workspaceSlug }: TKbRowProps) {
  const parentRef = useRef(null);
  const { getProjectById } = useProject();
  const { getUserDetails } = useMember();

  const owner = page.owned_by ? getUserDetails(page.owned_by) : undefined;

  return (
    <ListItem
      title={getPageName(page.name)}
      itemLink={`/${workspaceSlug}/projects/${projectId}/pages/${page.id}`}
      prependTitleElement={
        page.logo_props?.in_use ? (
          <Logo logo={page.logo_props} size={16} type="lucide" />
        ) : (
          <PagesOutline className="h-4 w-4 flex-shrink-0 text-tertiary" />
        )
      }
      appendTitleElement={
        <div className="flex items-center gap-3 text-11 text-tertiary">
          {page.project_ids.map((linkedId) => {
            const linkedProject = getProjectById(linkedId);
            if (!linkedProject) return null;
            return (
              <span key={linkedId} className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5">
                {linkedProject.name}
              </span>
            );
          })}
          <span className="rounded-sm bg-layer-2 px-1.5 py-0.5">{page.access === 1 ? "Private" : "Public"}</span>
          {owner && (
            <span className="hidden items-center gap-1.5 lg:flex">
              <AvatarGroupOverflow size="xs">
                <Avatar
                  alt={owner.display_name}
                  fallback={owner.display_name?.[0]?.toUpperCase()}
                  src={getFileURL(owner.avatar_url ?? "")}
                />
              </AvatarGroupOverflow>
              <span className="max-w-[100px] truncate">{owner.display_name}</span>
            </span>
          )}
          <span className="hidden md:inline">{renderFormattedDate(page.updated_at)}</span>
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

export const WorkspaceKbListView = observer(function WorkspaceKbListView({ typeFilter, projectFilter }: Props) {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const router = useAppRouter();
  const { workspaceProjectIds, getProjectById } = useProject();
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [pageState, setPageState] = useState(() => createKbPageState());
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingMore, setIsLoadingMore] = useState(false);

  const slug = workspaceSlug?.toString();
  const normalizedType = normalizeWorkspaceKbType(typeFilter);
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
    const params = buildWorkspaceKbParams({
      typeFilter: normalizedType,
      projectId: normalizedProjectFilter,
      search: debouncedSearch,
    });
    void pageService
      .getWorkspacePages(slug, params, { signal: controller.signal })
      .then((response) => setPageState(createKbPageState(normalizeKbResponse(response))))
      .catch(() => undefined)
      .finally(() => {
        if (!controller.signal.aborted) setIsLoading(false);
      });
    return () => controller.abort();
  }, [slug, normalizedType, normalizedProjectFilter, debouncedSearch]);

  const groups = useMemo(
    () => groupWorkspaceKbPages(pageState.results, (id) => getProjectById(id)?.name ?? id),
    [pageState.results, getProjectById]
  );

  const renderedGroups: TWorkspaceAggregateGroupItem[] = groups.map((group) => ({
    key: group.key,
    label: group.label,
    count: group.pages.length,
    items: (
      <>
        {group.pages.map((page) => (
          <KbRow
            key={page.id}
            page={page}
            projectId={
              normalizedProjectFilter && page.project_ids.includes(normalizedProjectFilter)
                ? normalizedProjectFilter
                : group.key
            }
            workspaceSlug={slug ?? ""}
          />
        ))}
      </>
    ),
  }));

  const buildTabHref = (nextType: TWorkspaceKbTab) => {
    if (!slug) return "#";
    const params = new URLSearchParams();
    if (nextType !== "all") params.set("type", nextType);
    if (normalizedProjectFilter) params.set("project", normalizedProjectFilter);
    const query = params.toString();
    return `/${slug}/pages${query ? `?${query}` : ""}`;
  };

  const handleProjectFilterChange = (projectId: string) => {
    if (!slug) return;
    const params = new URLSearchParams();
    if (normalizedType !== "all") params.set("type", normalizedType);
    if (projectId) params.set("project", projectId);
    const query = params.toString();
    router.push(`/${slug}/pages${query ? `?${query}` : ""}`);
  };

  const handleLoadMore = () => {
    if (!slug || !pageState.hasMore || !pageState.nextCursor || isLoadingMore) return;
    setIsLoadingMore(true);
    const params = {
      ...buildWorkspaceKbParams({
        typeFilter: normalizedType,
        projectId: normalizedProjectFilter,
        search: debouncedSearch,
      }),
      cursor: pageState.nextCursor,
    };
    void pageService
      .getWorkspacePages(slug, params, {})
      .then((response) => setPageState((state) => appendKbPage(state, normalizeKbResponse(response))))
      .catch(() => undefined)
      .finally(() => setIsLoadingMore(false));
  };

  return (
    <div className="relative flex min-h-0 w-full flex-1 flex-col">
      <div className="flex items-center gap-1 border-b border-subtle px-3 py-1.5">
        {KB_TAB_OPTIONS.map((tab) => (
          <button
            key={tab.key}
            type="button"
            onClick={() => router.push(buildTabHref(tab.key))}
            className={
              tab.key === normalizedType
                ? "rounded-sm bg-layer-3 px-2.5 py-1 text-12 font-medium text-primary"
                : "rounded-sm px-2.5 py-1 text-12 text-secondary hover:bg-layer-transparent-hover"
            }
          >
            {tab.label}
          </button>
        ))}
      </div>
      <WorkspaceAggregateList
        projectFilter={normalizedProjectFilter}
        onProjectFilterChange={handleProjectFilterChange}
        search={search}
        onSearchChange={setSearch}
        searchPlaceholder={t("service.catalog_search_placeholder")}
        groups={renderedGroups}
        totalCount={pageState.totalResults}
        isLoading={isLoading}
        emptyTitle={t("workspace_empty_state.kb.title")}
        emptyDescription={t("workspace_empty_state.kb.description")}
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
