import { set } from "lodash-es";
import { action, makeObservable, observable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
import type {
  IRelease,
  IReleaseChange,
  IReleaseDetail,
  TReleaseCreatePayload,
  TReleaseUpdatePayload,
} from "@plane/types";
// services
import { ReleaseService } from "@/services/release.service";
// store
import type { CoreRootStore } from "./root.store";

export interface IReleaseStore {
  loader: boolean;
  fetchedMap: Record<string, boolean>;
  releaseMap: Record<string, IRelease>;
  releaseIdsMap: Record<string, string[]>;
  detailMap: Record<string, IReleaseDetail>;
  errorMap: Record<string, boolean>;
  getReleaseById: (releaseId: string) => IRelease | null;
  getWorkspaceReleaseIds: (workspaceSlug: string) => string[] | null;
  getReleaseDetailById: (releaseId: string) => IReleaseDetail | null;
  getReleaseChanges: (releaseId: string) => IReleaseChange[] | null;
  fetchReleases: (workspaceSlug: string, params?: { status?: string }) => Promise<IRelease[] | undefined>;
  fetchReleaseDetail: (workspaceSlug: string, releaseId: string) => Promise<IReleaseDetail | undefined>;
  createRelease: (workspaceSlug: string, data: TReleaseCreatePayload) => Promise<IRelease>;
  updateRelease: (workspaceSlug: string, releaseId: string, data: TReleaseUpdatePayload) => Promise<IRelease>;
  deleteRelease: (workspaceSlug: string, releaseId: string) => Promise<void>;
  linkChanges: (workspaceSlug: string, releaseId: string, issueIds: string[]) => Promise<IReleaseChange[]>;
  unlinkChange: (workspaceSlug: string, releaseId: string, issueId: string) => Promise<void>;
}

export class ReleaseStore implements IReleaseStore {
  loader: boolean = false;
  fetchedMap: Record<string, boolean> = {};
  releaseMap: Record<string, IRelease> = {};
  releaseIdsMap: Record<string, string[]> = {};
  detailMap: Record<string, IReleaseDetail> = {};
  errorMap: Record<string, boolean> = {};
  rootStore: CoreRootStore;
  releaseService: ReleaseService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      fetchedMap: observable,
      releaseMap: observable,
      releaseIdsMap: observable,
      detailMap: observable,
      errorMap: observable,
      fetchReleases: action,
      fetchReleaseDetail: action,
      createRelease: action,
      updateRelease: action,
      deleteRelease: action,
      linkChanges: action,
      unlinkChange: action,
    });
    this.rootStore = _rootStore;
    this.releaseService = new ReleaseService();
  }

  getReleaseById = computedFn((releaseId: string) => this.releaseMap[releaseId] ?? null);

  getWorkspaceReleaseIds = computedFn((workspaceSlug: string) => {
    if (!this.fetchedMap[workspaceSlug]) return null;
    return this.releaseIdsMap[workspaceSlug] ?? [];
  });

  getReleaseDetailById = computedFn((releaseId: string) => this.detailMap[releaseId] ?? null);

  getReleaseChanges = computedFn((releaseId: string) => this.detailMap[releaseId]?.changes ?? null);

  fetchReleases = async (workspaceSlug: string, params?: { status?: string }) => {
    try {
      runInAction(() => {
        set(this.errorMap, workspaceSlug, false);
      });
      this.loader = true;
      const releases = await this.releaseService.getReleases(workspaceSlug, params);
      runInAction(() => {
        releases.forEach((release) => set(this.releaseMap, [release.id], release));
        set(
          this.releaseIdsMap,
          [workspaceSlug],
          releases.map((release) => release.id)
        );
        set(this.fetchedMap, [workspaceSlug], true);
        this.loader = false;
      });
      return releases;
    } catch {
      runInAction(() => {
        this.loader = false;
        set(this.errorMap, workspaceSlug, true);
      });
      return undefined;
    }
  };

  fetchReleaseDetail = async (workspaceSlug: string, releaseId: string) => {
    try {
      const release = await this.releaseService.getRelease(workspaceSlug, releaseId);
      runInAction(() => {
        set(this.releaseMap, [release.id], release);
        set(this.detailMap, [releaseId], release);
      });
      return release;
    } catch {
      return undefined;
    }
  };

  createRelease = async (workspaceSlug: string, data: TReleaseCreatePayload) => {
    const release = await this.releaseService.createRelease(workspaceSlug, data);
    runInAction(() => {
      set(this.releaseMap, [release.id], release);
      set(this.releaseIdsMap, [workspaceSlug], [release.id, ...(this.releaseIdsMap[workspaceSlug] ?? [])]);
    });
    return release;
  };

  updateRelease = async (workspaceSlug: string, releaseId: string, data: TReleaseUpdatePayload) => {
    const original = this.getReleaseById(releaseId);
    if (!original) throw new Error("Release not found");
    try {
      runInAction(() => {
        set(this.releaseMap, [releaseId], { ...original, ...data });
        const detail = this.detailMap[releaseId];
        if (detail) set(this.detailMap, [releaseId], { ...detail, ...data });
      });
      const response = await this.releaseService.updateRelease(workspaceSlug, releaseId, data);
      runInAction(() => {
        set(this.releaseMap, [releaseId], response);
        const detail = this.detailMap[releaseId];
        if (detail) set(this.detailMap, [releaseId], { ...detail, ...response });
      });
      return response;
    } catch (error) {
      runInAction(() => {
        set(this.releaseMap, [releaseId], original);
      });
      throw error;
    }
  };

  deleteRelease = async (workspaceSlug: string, releaseId: string) => {
    await this.releaseService.deleteRelease(workspaceSlug, releaseId);
    runInAction(() => {
      delete this.releaseMap[releaseId];
      delete this.detailMap[releaseId];
      Object.keys(this.releaseIdsMap).forEach((key) => {
        this.releaseIdsMap[key] = (this.releaseIdsMap[key] ?? []).filter((id) => id !== releaseId);
      });
    });
  };

  linkChanges = async (workspaceSlug: string, releaseId: string, issueIds: string[]) => {
    const changes = await this.releaseService.linkReleaseChanges(workspaceSlug, releaseId, issueIds);
    runInAction(() => {
      const detail = this.detailMap[releaseId];
      if (detail) set(this.detailMap, [releaseId], { ...detail, changes });
    });
    return changes;
  };

  unlinkChange = async (workspaceSlug: string, releaseId: string, issueId: string) => {
    await this.releaseService.unlinkReleaseChange(workspaceSlug, releaseId, issueId);
    runInAction(() => {
      const detail = this.detailMap[releaseId];
      if (detail)
        set(this.detailMap, [releaseId], {
          ...detail,
          changes: detail.changes.filter((change) => change.issue_id !== issueId),
        });
    });
  };
}
