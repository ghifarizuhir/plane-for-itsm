/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { action, makeObservable, observable } from "mobx";

export const WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY = "workspace_application_scope";

type TStorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

export interface IWorkspaceApplicationScopeStore {
  projectIdByWorkspace: Record<string, string | null>;
  getProjectId: (workspaceSlug: string) => string | null;
  setProjectId: (workspaceSlug: string, projectId: string | null) => void;
  reset: (workspaceSlug?: string) => void;
}

const getDefaultStorage = (): TStorageLike | null => (typeof window !== "undefined" ? window.localStorage : null);

export class WorkspaceApplicationScopeStore implements IWorkspaceApplicationScopeStore {
  projectIdByWorkspace: Record<string, string | null> = {};
  private storage: TStorageLike | null;

  constructor(storage: TStorageLike | null = getDefaultStorage()) {
    this.storage = storage;
    this.projectIdByWorkspace = this.readStorage();
    makeObservable(this, {
      projectIdByWorkspace: observable,
      setProjectId: action,
      reset: action,
    });
  }

  private readStorage = (): Record<string, string | null> => {
    if (!this.storage) return {};
    try {
      const raw = this.storage.getItem(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY);
      if (!raw) return {};
      const parsed: unknown = JSON.parse(raw);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
      return Object.fromEntries(
        Object.entries(parsed as Record<string, unknown>)
          .filter(([, value]) => typeof value === "string" || value === null)
          .map(([workspaceSlug, value]) => [workspaceSlug, value as string | null])
      );
    } catch {
      return {};
    }
  };

  private persist = () => {
    try {
      this.storage?.setItem(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY, JSON.stringify(this.projectIdByWorkspace));
    } catch {
      // Best-effort persistence; the in-memory scope still works for this session.
    }
  };

  getProjectId = (workspaceSlug: string) => this.projectIdByWorkspace[workspaceSlug] ?? null;

  setProjectId = (workspaceSlug: string, projectId: string | null) => {
    this.projectIdByWorkspace = { ...this.projectIdByWorkspace, [workspaceSlug]: projectId };
    this.persist();
  };

  reset = (workspaceSlug?: string) => {
    if (workspaceSlug) {
      const next = { ...this.projectIdByWorkspace };
      delete next[workspaceSlug];
      this.projectIdByWorkspace = next;
    } else {
      this.projectIdByWorkspace = {};
    }
    this.persist();
  };
}
