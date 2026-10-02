import { set } from "lodash-es";
import { action, makeObservable, observable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
import type {
  IReviewParticipant,
  IReviewRequest,
  IReviewRequestDetail,
  IReviewSession,
  IReviewSessionDetail,
  IReviewSessionItem,
  TReviewItemUpdatePayload,
  TReviewParticipantCreatePayload,
  TReviewParticipantUpdatePayload,
  TReviewRequestCreatePayload,
  TReviewRequestListParams,
  TReviewSessionCreatePayload,
  TReviewSessionListParams,
  TReviewSessionUpdatePayload,
} from "@plane/types";
// services
import { latestRequestForSubject, reviewRequestsKey, reviewSessionsKey } from "@/services/review.helpers";
import { ReviewService } from "@/services/review.service";
// store
import type { CoreRootStore } from "./root.store";

const sessionListKey = (params: TReviewSessionListParams) => `sessions:${reviewSessionsKey(params)}`;

export interface IReviewStore {
  loader: boolean;
  fetchedKeys: Record<string, boolean>;
  requestMap: Record<string, IReviewRequest>;
  requestIdsMap: Record<string, string[]>;
  requestDetailMap: Record<string, IReviewRequestDetail>;
  sessionMap: Record<string, IReviewSession>;
  sessionIdsMap: Record<string, string[]>;
  sessionDetailMap: Record<string, IReviewSessionDetail>;
  errorMap: Record<string, boolean>;
  getRequestById: (requestId: string) => IReviewRequest | null;
  getRequestIds: (params: TReviewRequestListParams) => string[] | null;
  getRequestDetailById: (requestId: string) => IReviewRequestDetail | null;
  getRequestsForChange: (changeIssueId: string) => IReviewRequest[];
  getRequestsForRelease: (releaseId: string) => IReviewRequest[];
  getLatestRequestForChange: (changeIssueId: string) => IReviewRequest | null;
  getLatestRequestForRelease: (releaseId: string) => IReviewRequest | null;
  getSessionById: (sessionId: string) => IReviewSession | null;
  getSessions: (params: TReviewSessionListParams) => IReviewSession[] | null;
  getSessionDetailById: (sessionId: string) => IReviewSessionDetail | null;
  fetchRequests: (workspaceSlug: string, params: TReviewRequestListParams) => Promise<IReviewRequest[] | undefined>;
  submitRequest: (workspaceSlug: string, data: TReviewRequestCreatePayload) => Promise<IReviewRequest>;
  withdrawRequest: (workspaceSlug: string, requestId: string) => Promise<IReviewRequest>;
  fetchRequestDetail: (workspaceSlug: string, requestId: string) => Promise<IReviewRequestDetail | undefined>;
  fetchSessions: (workspaceSlug: string, params: TReviewSessionListParams) => Promise<IReviewSession[] | undefined>;
  createSession: (workspaceSlug: string, data: TReviewSessionCreatePayload) => Promise<IReviewSession>;
  fetchSessionDetail: (workspaceSlug: string, sessionId: string) => Promise<IReviewSessionDetail | undefined>;
  updateSession: (
    workspaceSlug: string,
    sessionId: string,
    data: TReviewSessionUpdatePayload
  ) => Promise<IReviewSession>;
  completeSession: (workspaceSlug: string, sessionId: string) => Promise<IReviewSession>;
  cancelSession: (workspaceSlug: string, sessionId: string) => Promise<IReviewSession>;
  addSessionItems: (workspaceSlug: string, sessionId: string, requestIds: string[]) => Promise<IReviewSessionItem[]>;
  updateSessionItem: (
    workspaceSlug: string,
    sessionId: string,
    itemId: string,
    data: TReviewItemUpdatePayload
  ) => Promise<IReviewSessionItem>;
  removeSessionItem: (workspaceSlug: string, sessionId: string, itemId: string) => Promise<void>;
  addSessionParticipant: (
    workspaceSlug: string,
    sessionId: string,
    data: TReviewParticipantCreatePayload
  ) => Promise<IReviewParticipant>;
  updateSessionParticipant: (
    workspaceSlug: string,
    sessionId: string,
    participantId: string,
    data: TReviewParticipantUpdatePayload
  ) => Promise<IReviewParticipant>;
  removeSessionParticipant: (workspaceSlug: string, sessionId: string, participantId: string) => Promise<void>;
}

export class ReviewStore implements IReviewStore {
  loader: boolean = false;
  fetchedKeys: Record<string, boolean> = {};
  requestMap: Record<string, IReviewRequest> = {};
  requestIdsMap: Record<string, string[]> = {};
  requestDetailMap: Record<string, IReviewRequestDetail> = {};
  sessionMap: Record<string, IReviewSession> = {};
  sessionIdsMap: Record<string, string[]> = {};
  sessionDetailMap: Record<string, IReviewSessionDetail> = {};
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
      sessionMap: observable,
      sessionIdsMap: observable,
      sessionDetailMap: observable,
      errorMap: observable,
      fetchRequests: action,
      submitRequest: action,
      withdrawRequest: action,
      fetchRequestDetail: action,
      fetchSessions: action,
      createSession: action,
      fetchSessionDetail: action,
      updateSession: action,
      completeSession: action,
      cancelSession: action,
      addSessionItems: action,
      updateSessionItem: action,
      removeSessionItem: action,
      addSessionParticipant: action,
      updateSessionParticipant: action,
      removeSessionParticipant: action,
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

  getSessionById = computedFn((sessionId: string) => this.sessionMap[sessionId] ?? null);

  getSessions = computedFn((params: TReviewSessionListParams) => {
    const key = sessionListKey(params);
    if (!this.fetchedKeys[key]) return null;
    return (this.sessionIdsMap[key] ?? [])
      .map((sessionId) => this.sessionMap[sessionId])
      .filter((session): session is IReviewSession => Boolean(session));
  });

  getSessionDetailById = computedFn((sessionId: string) => this.sessionDetailMap[sessionId] ?? null);

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

  fetchSessions = async (workspaceSlug: string, params: TReviewSessionListParams) => {
    const key = sessionListKey(params);
    try {
      runInAction(() => {
        set(this.errorMap, key, false);
      });
      this.loader = true;
      const sessions = await this.reviewService.getReviewSessions(workspaceSlug, params);
      runInAction(() => {
        sessions.forEach((session) => set(this.sessionMap, [session.id], session));
        set(
          this.sessionIdsMap,
          [key],
          sessions.map((session) => session.id)
        );
        set(this.fetchedKeys, [key], true);
        this.loader = false;
      });
      return sessions;
    } catch {
      runInAction(() => {
        this.loader = false;
        set(this.errorMap, [key], true);
      });
      return undefined;
    }
  };

  createSession = async (workspaceSlug: string, data: TReviewSessionCreatePayload) => {
    const session = await this.reviewService.createReviewSession(workspaceSlug, data);
    runInAction(() => {
      set(this.sessionMap, [session.id], session);
    });
    return session;
  };

  fetchSessionDetail = async (workspaceSlug: string, sessionId: string) => {
    try {
      const session = await this.reviewService.getReviewSession(workspaceSlug, sessionId);
      runInAction(() => {
        set(this.sessionMap, [session.id], session);
        set(this.sessionDetailMap, [sessionId], session);
      });
      return session;
    } catch {
      return undefined;
    }
  };

  updateSession = async (workspaceSlug: string, sessionId: string, data: TReviewSessionUpdatePayload) => {
    const session = await this.reviewService.updateReviewSession(workspaceSlug, sessionId, data);
    runInAction(() => {
      set(this.sessionMap, [session.id], session);
      const detail = this.sessionDetailMap[sessionId];
      if (detail) set(this.sessionDetailMap, [sessionId], { ...detail, ...session });
    });
    return session;
  };

  completeSession = async (workspaceSlug: string, sessionId: string) => {
    const session = await this.reviewService.completeReviewSession(workspaceSlug, sessionId);
    runInAction(() => {
      set(this.sessionMap, [session.id], session);
      const detail = this.sessionDetailMap[sessionId];
      if (detail) set(this.sessionDetailMap, [sessionId], { ...detail, ...session });
    });
    return session;
  };

  cancelSession = async (workspaceSlug: string, sessionId: string) => {
    const session = await this.reviewService.cancelReviewSession(workspaceSlug, sessionId);
    runInAction(() => {
      set(this.sessionMap, [session.id], session);
      const detail = this.sessionDetailMap[sessionId];
      if (detail) set(this.sessionDetailMap, [sessionId], { ...detail, ...session });
    });
    return session;
  };

  addSessionItems = async (workspaceSlug: string, sessionId: string, requestIds: string[]) => {
    const items = await this.reviewService.addSessionItems(workspaceSlug, sessionId, requestIds);
    runInAction(() => {
      items.forEach((item) =>
        set(this.requestMap, [item.review_request_id], {
          ...this.requestMap[item.review_request_id],
          status: item.request_status,
        } as IReviewRequest)
      );
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        const existing = new Set(detail.items.map((item) => item.id));
        set(this.sessionDetailMap, [sessionId], {
          ...detail,
          items: [...detail.items, ...items.filter((item) => !existing.has(item.id))],
        });
      }
    });
    return items;
  };

  updateSessionItem = async (
    workspaceSlug: string,
    sessionId: string,
    itemId: string,
    data: TReviewItemUpdatePayload
  ) => {
    const item = await this.reviewService.updateSessionItem(workspaceSlug, sessionId, itemId, data);
    runInAction(() => {
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        set(this.sessionDetailMap, [sessionId], {
          ...detail,
          items: detail.items.map((existing) => (existing.id === item.id ? item : existing)),
        });
      }
    });
    return item;
  };

  removeSessionItem = async (workspaceSlug: string, sessionId: string, itemId: string) => {
    await this.reviewService.removeSessionItem(workspaceSlug, sessionId, itemId);
    runInAction(() => {
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        set(this.sessionDetailMap, [sessionId], {
          ...detail,
          items: detail.items.filter((item) => item.id !== itemId),
        });
      }
    });
  };

  addSessionParticipant = async (workspaceSlug: string, sessionId: string, data: TReviewParticipantCreatePayload) => {
    const participant = await this.reviewService.addSessionParticipant(workspaceSlug, sessionId, data);
    runInAction(() => {
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        const others = detail.participants.filter((existing) => existing.id !== participant.id);
        set(this.sessionDetailMap, [sessionId], { ...detail, participants: [...others, participant] });
      }
    });
    return participant;
  };

  updateSessionParticipant = async (
    workspaceSlug: string,
    sessionId: string,
    participantId: string,
    data: TReviewParticipantUpdatePayload
  ) => {
    const participant = await this.reviewService.updateSessionParticipant(
      workspaceSlug,
      sessionId,
      participantId,
      data
    );
    runInAction(() => {
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        set(this.sessionDetailMap, [sessionId], {
          ...detail,
          participants: detail.participants.map((existing) =>
            existing.id === participant.id ? participant : existing
          ),
        });
      }
    });
    return participant;
  };

  removeSessionParticipant = async (workspaceSlug: string, sessionId: string, participantId: string) => {
    await this.reviewService.removeSessionParticipant(workspaceSlug, sessionId, participantId);
    runInAction(() => {
      const detail = this.sessionDetailMap[sessionId];
      if (detail) {
        set(this.sessionDetailMap, [sessionId], {
          ...detail,
          participants: detail.participants.filter((participant) => participant.id !== participantId),
        });
      }
    });
  };
}
