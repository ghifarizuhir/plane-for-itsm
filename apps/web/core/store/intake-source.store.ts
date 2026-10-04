/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { set } from "lodash-es";
import { action, observable, makeObservable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type { TIntakeSource, TIntakeSourcePayload } from "@plane/types";
// services
import { IntakeSourceService } from "@/services/intake-source.service";
// store
import type { CoreRootStore } from "./root.store";

export interface IIntakeSourceStore {
  loader: boolean;
  fetchedMap: Record<string, boolean>;
  sourceMap: Record<string, TIntakeSource>;
  getSourcesByProject: (projectId: string) => TIntakeSource[] | null;
  fetchSources: (workspaceSlug: string, projectId: string) => Promise<void>;
  createSource: (workspaceSlug: string, projectId: string, data: TIntakeSourcePayload) => Promise<TIntakeSource>;
  updateSource: (
    workspaceSlug: string,
    projectId: string,
    sourceId: string,
    data: Partial<TIntakeSourcePayload> & { is_active?: boolean }
  ) => Promise<TIntakeSource>;
  deleteSource: (workspaceSlug: string, projectId: string, sourceId: string) => Promise<void>;
  rotateSource: (workspaceSlug: string, projectId: string, sourceId: string) => Promise<string>;
}

export class IntakeSourceStore implements IIntakeSourceStore {
  loader: boolean = false;
  fetchedMap: Record<string, boolean> = {};
  sourceMap: Record<string, TIntakeSource> = {};
  rootStore;
  intakeSourceService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      fetchedMap: observable,
      sourceMap: observable,
      fetchSources: action,
      createSource: action,
      updateSource: action,
      deleteSource: action,
      rotateSource: action,
    });
    this.rootStore = _rootStore;
    this.intakeSourceService = new IntakeSourceService();
  }

  getSourcesByProject = computedFn((projectId: string) => {
    if (!this.fetchedMap[projectId]) return null;
    return Object.values(this.sourceMap).filter((source) => source.project_id === projectId);
  });

  fetchSources = async (workspaceSlug: string, projectId: string) => {
    this.loader = true;
    try {
      const sources = await this.intakeSourceService.list(workspaceSlug, projectId);
      runInAction(() => {
        sources.forEach((source) => set(this.sourceMap, [source.id], source));
        set(this.fetchedMap, [projectId], true);
      });
    } finally {
      runInAction(() => {
        this.loader = false;
      });
    }
  };

  createSource = async (workspaceSlug: string, projectId: string, data: TIntakeSourcePayload) => {
    const source = await this.intakeSourceService.create(workspaceSlug, projectId, data);
    runInAction(() => set(this.sourceMap, [source.id], source));
    return source;
  };

  updateSource = async (
    workspaceSlug: string,
    projectId: string,
    sourceId: string,
    data: Partial<TIntakeSourcePayload> & { is_active?: boolean }
  ) => {
    const source = await this.intakeSourceService.update(workspaceSlug, projectId, sourceId, data);
    runInAction(() => set(this.sourceMap, [source.id], source));
    return source;
  };

  deleteSource = async (workspaceSlug: string, projectId: string, sourceId: string) => {
    await this.intakeSourceService.destroy(workspaceSlug, projectId, sourceId);
    runInAction(() => {
      delete this.sourceMap[sourceId];
    });
  };

  rotateSource = async (workspaceSlug: string, projectId: string, sourceId: string) => {
    const rotated = await this.intakeSourceService.rotate(workspaceSlug, projectId, sourceId);
    runInAction(() => set(this.sourceMap, [sourceId, "token"], rotated.token));
    return rotated.token;
  };
}
