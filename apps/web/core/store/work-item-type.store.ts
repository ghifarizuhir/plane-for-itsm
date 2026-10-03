/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// mobx
import { action, makeObservable, observable, runInAction } from "mobx";
// types
import type { TWorkItemType, TWorkItemTypePayload } from "@plane/types";
// services
import { WorkItemTypeService } from "@/services/work-item-type";
// store
import type { CoreRootStore } from "./root.store";

export interface IWorkItemTypeStore {
  // observables
  workItemTypes: TWorkItemType[] | undefined;
  // fetch actions
  fetchWorkItemTypes(workspaceSlug: string): Promise<TWorkItemType[]>;
  // crud actions
  createWorkItemType(workspaceSlug: string, data: TWorkItemTypePayload): Promise<TWorkItemType>;
  updateWorkItemType(
    workspaceSlug: string,
    typeId: string,
    data: Partial<TWorkItemTypePayload>
  ): Promise<TWorkItemType>;
  deleteWorkItemType(workspaceSlug: string, typeId: string): Promise<void>;
  importWorkItemTypes(workspaceSlug: string, projectId: string, typeIds: string[]): Promise<void>;
  unlinkWorkItemType(workspaceSlug: string, projectId: string, typeId: string): Promise<void>;
}

export class WorkItemTypeStore implements IWorkItemTypeStore {
  // observables
  workItemTypes: TWorkItemType[] | undefined = undefined;
  // services
  private service = new WorkItemTypeService();
  private _rootStore: CoreRootStore;

  constructor(_rootStore: CoreRootStore) {
    this._rootStore = _rootStore;
    makeObservable(this, {
      // observables
      workItemTypes: observable,
      // fetch actions
      fetchWorkItemTypes: action,
      // crud actions
      createWorkItemType: action,
      updateWorkItemType: action,
      deleteWorkItemType: action,
      importWorkItemTypes: action,
      unlinkWorkItemType: action,
    });
  }

  fetchWorkItemTypes = async (workspaceSlug: string) => {
    const types = await this.service.getWorkItemTypes(workspaceSlug);
    runInAction(() => {
      this.workItemTypes = types;
    });
    return types;
  };

  createWorkItemType = async (workspaceSlug: string, data: TWorkItemTypePayload) => {
    const type = await this.service.createWorkItemType(workspaceSlug, data);
    runInAction(() => {
      this.workItemTypes = [...(this.workItemTypes ?? []), type];
    });
    return type;
  };

  updateWorkItemType = async (workspaceSlug: string, typeId: string, data: Partial<TWorkItemTypePayload>) => {
    const type = await this.service.updateWorkItemType(workspaceSlug, typeId, data);
    runInAction(() => {
      this.workItemTypes = this.workItemTypes?.map((item) => (item.id === typeId ? type : item));
    });
    return type;
  };

  deleteWorkItemType = async (workspaceSlug: string, typeId: string) => {
    await this.service.deleteWorkItemType(workspaceSlug, typeId);
    runInAction(() => {
      this.workItemTypes = this.workItemTypes?.filter((item) => item.id !== typeId);
    });
  };

  importWorkItemTypes = async (workspaceSlug: string, projectId: string, typeIds: string[]) => {
    await this.service.importWorkItemTypes(workspaceSlug, projectId, typeIds);
  };

  unlinkWorkItemType = async (workspaceSlug: string, projectId: string, typeId: string) => {
    await this.service.unlinkWorkItemType(workspaceSlug, projectId, typeId);
  };
}
