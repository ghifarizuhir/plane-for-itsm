/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// mobx
import { action, makeObservable, observable, runInAction } from "mobx";
// types
import type {
  TWorkflow,
  TWorkflowMap,
  TWorkflowState,
  TWorkflowStatePayload,
  TWorkflowTransition,
  TWorkItemType,
  TWorkItemTypePayload,
} from "@plane/types";
// services
import { WorkflowService } from "@/services/workflow";
// store
import type { CoreRootStore } from "./root.store";

export interface IWorkflowStore {
  // observables
  workflows: TWorkflow[] | undefined;
  workItemTypes: TWorkItemType[] | undefined;
  workflowStates: Record<string, TWorkflowState[]>;
  workflowTransitions: Record<string, TWorkflowTransition[]>;
  workflowMap: Record<string, TWorkflowMap>;
  // per-project workflow-map refresh failure (read by the UI to show a warning toast)
  mapRefreshError: Record<string, "refresh_failed" | null>;
  // fetch actions
  fetchWorkflows(workspaceSlug: string): Promise<TWorkflow[]>;
  fetchWorkflowStates(workspaceSlug: string, workflowId: string): Promise<TWorkflowState[]>;
  fetchWorkflowTransitions(workspaceSlug: string, workflowId: string): Promise<TWorkflowTransition[]>;
  fetchWorkItemTypes(workspaceSlug: string): Promise<TWorkItemType[]>;
  fetchWorkflowMap(workspaceSlug: string, projectId: string): Promise<TWorkflowMap>;
  // crud actions
  createWorkflowState(workspaceSlug: string, workflowId: string, data: TWorkflowStatePayload): Promise<TWorkflowState>;
  updateWorkflowState(
    workspaceSlug: string,
    workflowId: string,
    stateId: string,
    data: Partial<TWorkflowStatePayload>
  ): Promise<TWorkflowState>;
  deleteWorkflowState(workspaceSlug: string, workflowId: string, stateId: string): Promise<void>;
  createWorkflowTransition(
    workspaceSlug: string,
    workflowId: string,
    data: { from_state_id: string; to_state_id: string }
  ): Promise<TWorkflowTransition>;
  deleteWorkflowTransition(workspaceSlug: string, workflowId: string, transitionId: string): Promise<void>;
  createWorkItemType(workspaceSlug: string, data: TWorkItemTypePayload): Promise<TWorkItemType>;
  updateWorkItemType(
    workspaceSlug: string,
    typeId: string,
    data: Partial<TWorkItemTypePayload>
  ): Promise<TWorkItemType>;
  deleteWorkItemType(workspaceSlug: string, typeId: string): Promise<void>;
  importWorkItemTypes(workspaceSlug: string, projectId: string, typeIds: string[]): Promise<void>;
  unlinkWorkItemType(workspaceSlug: string, projectId: string, typeId: string): Promise<void>;
  // computed
  getWorkflowMap(projectId: string): TWorkflowMap | undefined;
}

export class WorkflowStore implements IWorkflowStore {
  // observables
  workflows: TWorkflow[] | undefined = undefined;
  workItemTypes: TWorkItemType[] | undefined = undefined;
  workflowStates: Record<string, TWorkflowState[]> = {};
  workflowTransitions: Record<string, TWorkflowTransition[]> = {};
  workflowMap: Record<string, TWorkflowMap> = {};
  // per-project workflow-map refresh failure (read by the UI to show a warning toast)
  mapRefreshError: Record<string, "refresh_failed" | null> = {};
  // request sequencing so a slow response cannot overwrite a newer workflow map (plain field, not observable)
  private workflowMapRequestId: Record<string, number> = {};
  // same sequencing for the failure flag: an older failed refresh must not
  // re-flag a project whose newer refresh already succeeded
  private mapRefreshRequestId: Record<string, number> = {};
  // services
  private service = new WorkflowService();
  private _rootStore: CoreRootStore;

  constructor(_rootStore: CoreRootStore) {
    this._rootStore = _rootStore;
    makeObservable(this, {
      // observables
      workflows: observable,
      workItemTypes: observable,
      workflowStates: observable,
      workflowTransitions: observable,
      workflowMap: observable,
      mapRefreshError: observable,
      // fetch actions
      fetchWorkflows: action,
      fetchWorkflowStates: action,
      fetchWorkflowTransitions: action,
      fetchWorkItemTypes: action,
      fetchWorkflowMap: action,
      // crud actions
      createWorkflowState: action,
      updateWorkflowState: action,
      deleteWorkflowState: action,
      createWorkflowTransition: action,
      deleteWorkflowTransition: action,
      createWorkItemType: action,
      updateWorkItemType: action,
      deleteWorkItemType: action,
      importWorkItemTypes: action,
      unlinkWorkItemType: action,
    });
  }

  fetchWorkflows = async (workspaceSlug: string) => {
    const workflows = await this.service.getWorkflows(workspaceSlug);
    runInAction(() => {
      this.workflows = workflows;
    });
    return workflows;
  };

  fetchWorkflowStates = async (workspaceSlug: string, workflowId: string) => {
    const states = await this.service.getWorkflowStates(workspaceSlug, workflowId);
    runInAction(() => {
      this.workflowStates[workflowId] = states;
    });
    return states;
  };

  createWorkflowState = async (workspaceSlug: string, workflowId: string, data: TWorkflowStatePayload) => {
    const state = await this.service.createWorkflowState(workspaceSlug, workflowId, data);
    runInAction(() => {
      this.workflowStates[workflowId] = [...(this.workflowStates[workflowId] ?? []), state];
    });
    await this.refreshWorkflowMaps(workspaceSlug);
    return state;
  };

  updateWorkflowState = async (
    workspaceSlug: string,
    workflowId: string,
    stateId: string,
    data: Partial<TWorkflowStatePayload>
  ) => {
    const state = await this.service.updateWorkflowState(workspaceSlug, workflowId, stateId, data);
    runInAction(() => {
      this.workflowStates[workflowId] = (this.workflowStates[workflowId] ?? []).map((item) => {
        if (item.id === stateId) return state;
        // the API demotes the previous default state; mirror that so the UI never shows two defaults
        if (state.is_default) return Object.assign({}, item, { is_default: false });
        return item;
      });
    });
    await this.refreshWorkflowMaps(workspaceSlug);
    return state;
  };

  deleteWorkflowState = async (workspaceSlug: string, workflowId: string, stateId: string) => {
    await this.service.deleteWorkflowState(workspaceSlug, workflowId, stateId);
    runInAction(() => {
      this.workflowStates[workflowId] = (this.workflowStates[workflowId] ?? []).filter((item) => item.id !== stateId);
      this.workflowTransitions[workflowId] = (this.workflowTransitions[workflowId] ?? []).filter(
        (transition) => transition.from_state_id !== stateId && transition.to_state_id !== stateId
      );
    });
    await this.refreshWorkflowMaps(workspaceSlug);
  };

  fetchWorkflowTransitions = async (workspaceSlug: string, workflowId: string) => {
    const transitions = await this.service.getWorkflowTransitions(workspaceSlug, workflowId);
    runInAction(() => {
      this.workflowTransitions[workflowId] = transitions;
    });
    return transitions;
  };

  createWorkflowTransition = async (
    workspaceSlug: string,
    workflowId: string,
    data: { from_state_id: string; to_state_id: string }
  ) => {
    const transition = await this.service.createWorkflowTransition(workspaceSlug, workflowId, data);
    runInAction(() => {
      this.workflowTransitions[workflowId] = [...(this.workflowTransitions[workflowId] ?? []), transition];
    });
    await this.refreshWorkflowMaps(workspaceSlug);
    return transition;
  };

  deleteWorkflowTransition = async (workspaceSlug: string, workflowId: string, transitionId: string) => {
    await this.service.deleteWorkflowTransition(workspaceSlug, workflowId, transitionId);
    runInAction(() => {
      this.workflowTransitions[workflowId] = (this.workflowTransitions[workflowId] ?? []).filter(
        (item) => item.id !== transitionId
      );
    });
    await this.refreshWorkflowMaps(workspaceSlug);
  };

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
    await this.refreshWorkflowMaps(workspaceSlug, type.project_ids);
    return type;
  };

  updateWorkItemType = async (workspaceSlug: string, typeId: string, data: Partial<TWorkItemTypePayload>) => {
    const type = await this.service.updateWorkItemType(workspaceSlug, typeId, data);
    runInAction(() => {
      this.workItemTypes = this.workItemTypes?.map((item) => (item.id === typeId ? type : item));
    });
    await this.refreshWorkflowMaps(workspaceSlug, type.project_ids);
    return type;
  };

  deleteWorkItemType = async (workspaceSlug: string, typeId: string) => {
    const projectIds = this.workItemTypes?.find((item) => item.id === typeId)?.project_ids ?? [];
    await this.service.deleteWorkItemType(workspaceSlug, typeId);
    runInAction(() => {
      this.workItemTypes = this.workItemTypes?.filter((item) => item.id !== typeId);
    });
    await this.refreshWorkflowMaps(workspaceSlug, projectIds);
  };

  importWorkItemTypes = async (workspaceSlug: string, projectId: string, typeIds: string[]) => {
    await this.service.importWorkItemTypes(workspaceSlug, projectId, typeIds);
    await this.refreshWorkflowMaps(workspaceSlug, [projectId]);
    await this._rootStore.state.fetchProjectStates(workspaceSlug, projectId).catch(() => undefined);
  };

  unlinkWorkItemType = async (workspaceSlug: string, projectId: string, typeId: string) => {
    await this.service.unlinkWorkItemType(workspaceSlug, projectId, typeId);
    await this.refreshWorkflowMaps(workspaceSlug, [projectId]);
    await this._rootStore.state.fetchProjectStates(workspaceSlug, projectId).catch(() => undefined);
  };

  fetchWorkflowMap = async (workspaceSlug: string, projectId: string) => {
    const requestId = (this.workflowMapRequestId[projectId] ?? 0) + 1;
    this.workflowMapRequestId[projectId] = requestId;
    const map = await this.service.getWorkflowMap(workspaceSlug, projectId);
    runInAction(() => {
      // only the latest in-flight request for this project may write the map
      if (this.workflowMapRequestId[projectId] === requestId) this.workflowMap[projectId] = map;
    });
    return map;
  };

  private refreshWorkflowMaps = async (workspaceSlug: string, projectIds?: string[]) => {
    const targets = projectIds
      ? projectIds.filter((projectId) => this.workflowMap[projectId] !== undefined)
      : Object.keys(this.workflowMap);
    await Promise.all(
      targets.map(async (projectId) => {
        const requestId = (this.mapRefreshRequestId[projectId] ?? 0) + 1;
        this.mapRefreshRequestId[projectId] = requestId;
        try {
          await this.fetchWorkflowMap(workspaceSlug, projectId);
          runInAction(() => {
            // only the latest refresh for this project may clear the flag
            if (this.mapRefreshRequestId[projectId] === requestId) this.mapRefreshError[projectId] = null;
          });
        } catch {
          runInAction(() => {
            // and only the latest failed refresh may raise it
            if (this.mapRefreshRequestId[projectId] === requestId) this.mapRefreshError[projectId] = "refresh_failed";
          });
        }
      })
    );
  };

  getWorkflowMap = (projectId: string) => this.workflowMap[projectId];
}
