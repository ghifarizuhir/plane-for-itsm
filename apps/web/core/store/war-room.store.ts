/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { set } from "lodash-es";
import { action, observable, makeObservable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type {
  IWarRoom,
  IWarRoomListItem,
  IWarRoomSummary,
  TWarRoomCreatePayload,
  TWarRoomListParams,
} from "@plane/types";
// helpers
import { isActiveWarRoomStatus } from "@/services/war-room.helpers";
// services
import { WarRoomService } from "@/services/war-room.service";
// store
import type { CoreRootStore } from "./root.store";

export interface IWarRoomStore {
  loader: boolean;
  fetchedMap: Record<string, boolean>;
  warRoomMap: Record<string, IWarRoomListItem>;
  warRoomIdsMap: Record<string, string[]>;
  detailMap: Record<string, IWarRoom>;
  summaryMap: Record<string, IWarRoomSummary>;
  errorMap: Record<string, boolean>;
  detailErrorMap: Record<string, boolean>;
  getWarRoomById: (warRoomId: string) => IWarRoomListItem | null;
  getProjectWarRoomIds: (projectId: string) => string[] | null;
  getWarRoomDetailById: (warRoomId: string) => IWarRoom | null;
  getProjectSummary: (projectId: string) => IWarRoomSummary | null;
  getActiveWarRoomByIssue: (projectId: string, issueId: string) => IWarRoomListItem | null;
  fetchWarRooms: (
    workspaceSlug: string,
    projectId: string,
    params?: TWarRoomListParams
  ) => Promise<IWarRoomListItem[] | undefined>;
  fetchWarRoomSummary: (workspaceSlug: string, projectId: string) => Promise<IWarRoomSummary | undefined>;
  fetchWarRoomDetail: (workspaceSlug: string, projectId: string, warRoomId: string) => Promise<IWarRoom | undefined>;
  createWarRoom: (workspaceSlug: string, projectId: string, data: TWarRoomCreatePayload) => Promise<IWarRoom>;
}

export class WarRoomStore implements IWarRoomStore {
  loader: boolean = false;
  fetchedMap: Record<string, boolean> = {};
  warRoomMap: Record<string, IWarRoomListItem> = {};
  warRoomIdsMap: Record<string, string[]> = {};
  detailMap: Record<string, IWarRoom> = {};
  summaryMap: Record<string, IWarRoomSummary> = {};
  errorMap: Record<string, boolean> = {};
  detailErrorMap: Record<string, boolean> = {};
  rootStore: CoreRootStore;
  warRoomService: WarRoomService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      fetchedMap: observable,
      warRoomMap: observable,
      warRoomIdsMap: observable,
      detailMap: observable,
      summaryMap: observable,
      errorMap: observable,
      detailErrorMap: observable,
      fetchWarRooms: action,
      fetchWarRoomSummary: action,
      fetchWarRoomDetail: action,
      createWarRoom: action,
    });
    this.rootStore = _rootStore;
    this.warRoomService = new WarRoomService();
  }

  getWarRoomById = computedFn((warRoomId: string) => this.warRoomMap[warRoomId] ?? null);

  getProjectWarRoomIds = computedFn((projectId: string) =>
    this.fetchedMap[projectId] ? (this.warRoomIdsMap[projectId] ?? []) : null
  );

  getWarRoomDetailById = computedFn((warRoomId: string) => this.detailMap[warRoomId] ?? null);

  getProjectSummary = computedFn((projectId: string) => this.summaryMap[projectId] ?? null);

  getActiveWarRoomByIssue = computedFn((projectId: string, issueId: string) => {
    const room = Object.values(this.warRoomMap).find(
      (candidate) =>
        candidate.project_id === projectId &&
        candidate.primary_issue_id === issueId &&
        isActiveWarRoomStatus(candidate.status)
    );
    return room ?? null;
  });

  fetchWarRooms = async (workspaceSlug: string, projectId: string, params?: TWarRoomListParams) => {
    try {
      runInAction(() => {
        set(this.errorMap, projectId, false);
        this.loader = true;
      });
      const rooms = await this.warRoomService.getWarRooms(workspaceSlug, projectId, params);
      runInAction(() => {
        rooms.forEach((room) => set(this.warRoomMap, [room.id], room));
        set(
          this.warRoomIdsMap,
          projectId,
          rooms.map((room) => room.id)
        );
        set(this.fetchedMap, projectId, true);
        this.loader = false;
      });
      return rooms;
    } catch {
      runInAction(() => {
        this.loader = false;
        set(this.errorMap, projectId, true);
      });
      return undefined;
    }
  };

  fetchWarRoomSummary = async (workspaceSlug: string, projectId: string) => {
    try {
      const summary = await this.warRoomService.getWarRoomSummary(workspaceSlug, projectId);
      runInAction(() => {
        set(this.summaryMap, projectId, summary);
      });
      return summary;
    } catch {
      return undefined;
    }
  };

  fetchWarRoomDetail = async (workspaceSlug: string, projectId: string, warRoomId: string) => {
    try {
      runInAction(() => {
        set(this.detailErrorMap, warRoomId, false);
      });
      const room = await this.warRoomService.getWarRoom(workspaceSlug, projectId, warRoomId);
      runInAction(() => {
        set(this.detailMap, [warRoomId], room);
      });
      return room;
    } catch {
      runInAction(() => {
        set(this.detailErrorMap, warRoomId, true);
      });
      return undefined;
    }
  };

  createWarRoom = async (workspaceSlug: string, projectId: string, data: TWarRoomCreatePayload) => {
    const room = await this.warRoomService.createWarRoom(workspaceSlug, projectId, data);
    runInAction(() => {
      set(this.detailMap, [room.id], room);
      const currentIds = this.warRoomIdsMap[projectId] ?? [];
      set(this.warRoomIdsMap, projectId, [room.id, ...currentIds.filter((id) => id !== room.id)]);
      set(this.fetchedMap, projectId, true);
    });
    return room;
  };
}
