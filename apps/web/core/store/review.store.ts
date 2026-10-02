import { set } from "lodash-es";
import { action, makeObservable, observable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
import type {
  IReviewRequest,
  IReviewRequestDetail,
  TReviewRequestCreatePayload,
  TReviewRequestListParams,
} from "@plane/types";
// services
import { latestRequestForSubject, reviewRequestsKey } from "@/services/review.helpers";
import { ReviewService } from "@/services/review.service";
// store
import type { CoreRootStore } from "./root.store";

export interface IReviewStore {
  loader: boolean;
  fetchedKeys: Record<string, boolean>;
  requestMap: Record<string, IReviewRequest>;
  requestIdsMap: Record<string, string[]>;
  requestDetailMap: Record<string, IReviewRequestDetail>;
  errorMap: Record<string, boolean>;
  getRequestById: (requestId: string) => IReviewRequest | null;
  getRequestIds: (params: TReviewRequestListParams) => string[] | null;
  getRequestDetailById: (requestId: string) => IReviewRequestDetail | null;
  getRequestsForChange: (changeIssueId: string) => IReviewRequest[];
  getRequestsForRelease: (releaseId: string) => IReviewRequest[];
  getLatestRequestForChange: (changeIssueId: string) => IReviewRequest | null;
  getLatestRequestForRelease: (releaseId: string) => IReviewRequest | null;
  fetchRequests: (workspaceSlug: string, params: TReviewRequestListParams) => Promise<IReviewRequest[] | undefined>;
  submitRequest: (workspaceSlug: string, data: TReviewRequestCreatePayload) => Promise<IReviewRequest>;
  withdrawRequest: (workspaceSlug: string, requestId: string) => Promise<IReviewRequest>;
  fetchRequestDetail: (workspaceSlug: string, requestId: string) => Promise<IReviewRequestDetail | undefined>;
}

export class ReviewStore implements IReviewStore {
  loader: boolean = false;
  fetchedKeys: Record<string, boolean> = {};
  requestMap: Record<string, IReviewRequest> = {};
  requestIdsMap: Record<string, string[]> = {};
  requestDetailMap: Record<string, IReviewRequestDetail> = {};
  errorMap: Record<string, boolean> = {};
  rootStore: CoreRootStore;
  reviewService: ReviewService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      fetchedKeys: observable,
      requestMap: observable,
      requestIdsMap: observable,
      requestDetailMap: observable,
      errorMap: observable,
      fetchRequests: action,
      submitRequest: action,
      withdrawRequest: action,
      fetchRequestDetail: action,
    });
    this.rootStore = _rootStore;
    this.reviewService = new ReviewService();
  }

  getRequestById = computedFn((requestId: string) => this.requestMap[requestId] ?? null);

  getRequestIds = computedFn((params: TReviewRequestListParams) => {
    const key = reviewRequestsKey(params);
    if (!this.fetchedKeys[key]) return null;
    return this.requestIdsMap[key] ?? [];
  });

  getRequestDetailById = computedFn((requestId: string) => this.requestDetailMap[requestId] ?? null);

  getRequestsForChange = computedFn((changeIssueId: string) =>
    Object.values(this.requestMap).filter((request) => request.change_issue_id === changeIssueId)
  );

  getRequestsForRelease = computedFn((releaseId: string) =>
    Object.values(this.requestMap).filter((request) => request.release_id === releaseId)
  );

  getLatestRequestForChange = computedFn((changeIssueId: string) =>
    latestRequestForSubject(
      this.getRequestsForChange(changeIssueId).filter((request) => request.status !== "withdrawn")
    )
  );

  getLatestRequestForRelease = computedFn((releaseId: string) =>
    latestRequestForSubject(this.getRequestsForRelease(releaseId).filter((request) => request.status !== "withdrawn"))
  );

  fetchRequests = async (workspaceSlug: string, params: TReviewRequestListParams) => {
    const key = reviewRequestsKey(params);
    try {
      runInAction(() => {
        set(this.errorMap, key, false);
      });
      this.loader = true;
      const requests = await this.reviewService.getReviewRequests(workspaceSlug, params);
      runInAction(() => {
        requests.forEach((request) => set(this.requestMap, [request.id], request));
        set(
          this.requestIdsMap,
          [key],
          requests.map((request) => request.id)
        );
        set(this.fetchedKeys, [key], true);
        this.loader = false;
      });
      return requests;
    } catch {
      runInAction(() => {
        this.loader = false;
        set(this.errorMap, key, true);
      });
      return undefined;
    }
  };

  submitRequest = async (workspaceSlug: string, data: TReviewRequestCreatePayload) => {
    const request = await this.reviewService.submitReviewRequest(workspaceSlug, data);
    runInAction(() => {
      set(this.requestMap, [request.id], request);
    });
    return request;
  };

  withdrawRequest = async (workspaceSlug: string, requestId: string) => {
    const request = await this.reviewService.withdrawReviewRequest(workspaceSlug, requestId);
    runInAction(() => {
      set(this.requestMap, [request.id], request);
      if (this.requestDetailMap[requestId])
        set(this.requestDetailMap, [requestId], { ...this.requestDetailMap[requestId], ...request });
    });
    return request;
  };

  fetchRequestDetail = async (workspaceSlug: string, requestId: string) => {
    try {
      const request = await this.reviewService.getReviewRequest(workspaceSlug, requestId);
      runInAction(() => {
        set(this.requestMap, [request.id], request);
        set(this.requestDetailMap, [requestId], request);
      });
      return request;
    } catch {
      return undefined;
    }
  };
}
