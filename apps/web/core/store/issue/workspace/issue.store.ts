/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { action, makeObservable, runInAction } from "mobx";
// base class
import type {
  IssuePaginationOptions,
  TBulkOperationsPayload,
  TIssue,
  TIssuesResponse,
  TLoader,
  ViewFlags,
} from "@plane/types";
// services
import { WorkspaceService } from "@/services/workspace.service";
// helpers
import { appendRichFilterNode } from "@/lib/issue-filter-scope";
import type { TWorkspaceIssueRow } from "@/lib/workspace-issue-grouping";
import {
  buildGroupedWorkspaceIssuesResponse,
  filterWorkspaceIssuesByDateRange,
  resolveWorkspaceIssueGrouping,
} from "@/lib/workspace-issue-grouping";
// types
import type { IBaseIssuesStore } from "../helpers/base-issues.store";
import { BaseIssuesStore } from "../helpers/base-issues.store";
import type { IIssueRootStore } from "../root.store";
import type { IWorkspaceIssuesFilter } from "./filter.store";

export interface IWorkspaceIssues extends IBaseIssuesStore {
  // observable
  viewFlags: ViewFlags;
  // actions
  fetchIssues: (
    workspaceSlug: string,
    viewId: string,
    loadType: TLoader,
    options: IssuePaginationOptions
  ) => Promise<TIssuesResponse | undefined>;
  fetchIssuesWithExistingPagination: (
    workspaceSlug: string,
    viewId: string,
    loadType: TLoader
  ) => Promise<TIssuesResponse | undefined>;
  fetchNextIssues: (
    workspaceSlug: string,
    viewId: string,
    groupId?: string,
    subGroupId?: string
  ) => Promise<TIssuesResponse | undefined>;

  createIssue: (workspaceSlug: string, projectId: string, data: Partial<TIssue>) => Promise<TIssue>;
  updateIssue: (workspaceSlug: string, projectId: string, issueId: string, data: Partial<TIssue>) => Promise<void>;
  archiveIssue: (workspaceSlug: string, projectId: string, issueId: string) => Promise<void>;
  removeBulkIssues: (workspaceSlug: string, projectId: string, issueIds: string[]) => Promise<void>;
  archiveBulkIssues: (workspaceSlug: string, projectId: string, issueIds: string[]) => Promise<void>;
  bulkUpdateProperties: (workspaceSlug: string, projectId: string, data: TBulkOperationsPayload) => Promise<void>;

  quickAddIssue: undefined;
  clear(): void;
}

export class WorkspaceIssues extends BaseIssuesStore implements IWorkspaceIssues {
  viewFlags = {
    enableQuickAdd: true,
    enableIssueCreation: true,
    enableInlineEditing: true,
  };
  // service
  workspaceService;
  // filterStore
  issueFilterStore;

  constructor(_rootStore: IIssueRootStore, issueFilterStore: IWorkspaceIssuesFilter) {
    super(_rootStore, issueFilterStore);

    makeObservable(this, {
      // action
      fetchIssues: action,
      fetchNextIssues: action,
      fetchIssuesWithExistingPagination: action,
    });
    // services
    this.workspaceService = new WorkspaceService();
    // filter store
    this.issueFilterStore = issueFilterStore;
  }

  fetchParentStats = () => {};

  /** */
  updateParentStats = () => {};

  /**
   * This method is called to fetch the first issues of pagination
   * @param workspaceSlug
   * @param viewId
   * @param loadType
   * @param options
   * @returns
   */
  fetchIssues = async (
    workspaceSlug: string,
    viewId: string,
    loadType: TLoader,
    options: IssuePaginationOptions,
    isExistingPaginationOptions: boolean = false
  ) => {
    try {
      // set loader and clear store
      runInAction(() => {
        this.setLoader(loadType);
      });
      this.clear(!isExistingPaginationOptions);

      // get params from pagination options (clone: computedFn may memoize it)
      const params = {
        ...this.issueFilterStore?.getFilterParams(options, viewId, undefined, undefined, undefined),
      };

      // Calendar range filter: the workspace endpoint ignores the legacy
      // `target_date=after;before` param, so push it through the rich filter
      // tree (supported allowlist key) instead.
      if (options.after && options.before) {
        params.filters = appendRichFilterNode(params.filters, {
          target_date__range: `${options.after},${options.before}`,
        });
      }

      // The sidebar application scope is a workspace-wide project filter. The
      // workspace endpoint ignores legacy `?project=`, so scope via rich filters.
      const scopedProjectId = this.rootIssueStore.rootStore.workspaceApplicationScope.getProjectId(workspaceSlug);
      if (scopedProjectId) {
        params.filters = appendRichFilterNode(params.filters, { project_id: [scopedProjectId] });
      }

      // Grouped layouts (list/kanban/calendar) need a single generous page:
      // the workspace endpoint paginates flat, not per group.
      const displayFilters = this.issueFilterStore?.getIssueFilters(viewId)?.displayFilters;
      const { groupBy, subGroupBy } = options.canGroup
        ? resolveWorkspaceIssueGrouping(displayFilters?.layout, displayFilters?.group_by, displayFilters?.sub_group_by)
        : { groupBy: null, subGroupBy: null };

      if (groupBy) {
        params.per_page = "200";
        params.cursor = "200:0:0";
      }

      // call the fetch issues API with the params
      const response = await this.workspaceService.getViewIssues(workspaceSlug, params, {
        signal: this.controller.signal,
      });

      // The workspace API always returns flat rows — build groups on the
      // client so board/grouped-list/calendar can consume them.
      let normalizedResponse = response;
      if (groupBy && Array.isArray(response?.results)) {
        const rows = filterWorkspaceIssuesByDateRange(
          response.results as unknown as TWorkspaceIssueRow[],
          options.after,
          options.before
        );
        normalizedResponse = buildGroupedWorkspaceIssuesResponse(
          { ...response, results: rows as unknown as TIssuesResponse["results"] },
          groupBy,
          subGroupBy
        );
      }

      // after fetching issues, call the base method to process the response further
      this.onfetchIssues(
        normalizedResponse,
        options,
        workspaceSlug,
        undefined,
        undefined,
        !isExistingPaginationOptions
      );
      return normalizedResponse;
    } catch (error) {
      // set loader to undefined if errored out
      this.setLoader(undefined);
      throw error;
    }
  };

  /**
   * This method is called subsequent pages of pagination
   * if groupId/subgroupId is provided, only that specific group's next page is fetched
   * else all the groups' next page is fetched
   * @param workspaceSlug
   * @param viewId
   * @param groupId
   * @param subGroupId
   * @returns
   */
  fetchNextIssues = async (workspaceSlug: string, viewId: string, groupId?: string, subGroupId?: string) => {
    // Grouped workspace layouts render from a single client-grouped page;
    // the flat endpoint cannot paginate per group correctly.
    const displayFilters = this.issueFilterStore?.getIssueFilters(viewId)?.displayFilters;
    const { groupBy } = resolveWorkspaceIssueGrouping(
      displayFilters?.layout,
      displayFilters?.group_by,
      displayFilters?.sub_group_by
    );
    if (groupBy) return;

    const cursorObject = this.getPaginationData(groupId, subGroupId);
    // if there are no pagination options and the next page results do not exist the return
    if (!this.paginationOptions || (cursorObject && !cursorObject?.nextPageResults)) return;
    try {
      // set Loader
      this.setLoader("pagination", groupId, subGroupId);

      // get params from stored pagination options
      const params = {
        ...this.issueFilterStore?.getFilterParams(
          this.paginationOptions,
          viewId,
          this.getNextCursor(groupId, subGroupId),
          groupId,
          subGroupId
        ),
      };

      // Keep paginated pages scoped to the sidebar application selection.
      const scopedProjectId = this.rootIssueStore.rootStore.workspaceApplicationScope.getProjectId(workspaceSlug);
      if (scopedProjectId) {
        params.filters = appendRichFilterNode(params.filters, { project_id: [scopedProjectId] });
      }

      // call the fetch issues API with the params for next page in issues
      const response = await this.workspaceService.getViewIssues(workspaceSlug, params);

      // after the next page of issues are fetched, call the base method to process the response
      this.onfetchNexIssues(response, groupId, subGroupId);
      return response;
    } catch (error) {
      // set Loader as undefined if errored out
      this.setLoader(undefined, groupId, subGroupId);
      throw error;
    }
  };

  /**
   * This Method exists to fetch the first page of the issues with the existing stored pagination
   * This is useful for refetching when filters, groupBy, orderBy etc changes
   * @param workspaceSlug
   * @param viewId
   * @param loadType
   * @returns
   */
  fetchIssuesWithExistingPagination = async (workspaceSlug: string, viewId: string, loadType: TLoader) => {
    if (!this.paginationOptions) return;
    return await this.fetchIssues(workspaceSlug, viewId, loadType, this.paginationOptions, true);
  };

  // Using aliased names as they cannot be overridden in other stores
  archiveBulkIssues = this.bulkArchiveIssues;
  updateIssue = this.issueUpdate;
  archiveIssue = this.issueArchive;

  // Setting them as undefined as they can not performed on workspace issues
  quickAddIssue = undefined;
}
