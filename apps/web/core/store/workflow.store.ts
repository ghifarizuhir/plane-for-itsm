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
  TWorkflowPayload,
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
  // fetch actions
  fetchWorkflows(workspaceSlug: string): Promise<TWorkflow[]>;
  fetchWorkflowStates(workspaceSlug: string, workflowId: string): Promise<TWorkflowState[]>;
  fetchWorkflowTransitions(workspaceSlug: string, workflowId: string): Promise<TWorkflowTransition[]>;
  fetchWorkItemTypes(workspaceSlug: string): Promise<TWorkItemType[]>;
  fetchWorkflowMap(workspaceSlug: string, projectId: string): Promise<TWorkflowMap>;
  // crud actions
  createWorkflow(workspaceSlug: string, data: TWorkflowPayload): Promise<TWorkflow>;
  updateWorkflow(workspaceSlug: string, workflowId: string, data: Partial<TWorkflowPayload>): Promise<TWorkflow>;
  deleteWorkflow(workspaceSlug: string, workflowId: string): Promise<void>;
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
  // services
  private service = new WorkflowService();

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      // observables
      workflows: observable,
      workItemTypes: observable,
      workflowStates: observable,
      workflowTransitions: observable,
      workflowMap: observable,
      // fetch actions
      fetchWorkflows: action,
      fetchWorkflowStates: action,
      fetchWorkflowTransitions: action,
      fetchWorkItemTypes: action,
      fetchWorkflowMap: action,
      // crud actions
      createWorkflow: action,
      updateWorkflow: action,
      deleteWorkflow: action,
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

  createWorkflow = async (workspaceSlug: string, data: TWorkflowPayload) => {
    const workflow = await this.service.createWorkflow(workspaceSlug, data);
    runInAction(() => {
      this.workflows = [...(this.workflows ?? []), workflow];
    });
    return workflow;
  };

  updateWorkflow = async (workspaceSlug: string, workflowId: string, data: Partial<TWorkflowPayload>) => {
    const workflow = await this.service.updateWorkflow(workspaceSlug, workflowId, data);
    runInAction(() => {
      this.workflows = this.workflows?.map((item) => (item.id === workflowId ? workflow : item));
    });
    return workflow;
  };

  deleteWorkflow = async (workspaceSlug: string, workflowId: string) => {
    await this.service.deleteWorkflow(workspaceSlug, workflowId);
    runInAction(() => {
      this.workflows = this.workflows?.filter((item) => item.id !== workflowId);
      delete this.workflowStates[workflowId];
      delete this.workflowTransitions[workflowId];
    });
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
      this.workflowStates[workflowId] = (this.workflowStates[workflowId] ?? []).map((item) =>
        item.id === stateId ? state : item
      );
    });
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
    return transition;
  };

  deleteWorkflowTransition = async (workspaceSlug: string, workflowId: string, transitionId: string) => {
    await this.service.deleteWorkflowTransition(workspaceSlug, workflowId, transitionId);
    runInAction(() => {
      this.workflowTransitions[workflowId] = (this.workflowTransitions[workflowId] ?? []).filter(
        (item) => item.id !== transitionId
      );
    });
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
    await this.fetchWorkflowMap(workspaceSlug, projectId);
  };

  unlinkWorkItemType = async (workspaceSlug: string, projectId: string, typeId: string) => {
    await this.service.unlinkWorkItemType(workspaceSlug, projectId, typeId);
    await this.fetchWorkflowMap(workspaceSlug, projectId);
  };

  fetchWorkflowMap = async (workspaceSlug: string, projectId: string) => {
    const map = await this.service.getWorkflowMap(workspaceSlug, projectId);
    runInAction(() => {
      this.workflowMap[projectId] = map;
    });
    return map;
  };

  getWorkflowMap = (projectId: string) => this.workflowMap[projectId];
}
