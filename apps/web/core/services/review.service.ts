import { API_BASE_URL } from "@plane/constants";
import type {
  IReviewParticipant,
  IReviewRequest,
  IReviewRequestDetail,
  IReviewSession,
  IReviewSessionBriefing,
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
import { APIService } from "@/services/api.service";

type TReviewErrorBody = { detail?: string; error?: string; [key: string]: unknown };

const toReviewError = (error: unknown): Error => {
  const body = (error as { response?: { data?: TReviewErrorBody } })?.response?.data;
  let fieldMessage: string | undefined;
  if (body && typeof body === "object") {
    const first = Object.values(body).find((value) => typeof value === "string");
    fieldMessage = typeof first === "string" ? first : undefined;
  }
  const message = body?.detail ?? body?.error ?? fieldMessage ?? "Something went wrong. Please try again.";
  const normalized = new Error(message) as Error & { detail?: string; error?: string };
  normalized.detail = body?.detail;
  normalized.error = body?.error;
  return normalized;
};

export class ReviewService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  private workspacePath(workspaceSlug: string): string {
    return `/api/workspaces/${workspaceSlug}`;
  }

  async getReviewRequests(workspaceSlug: string, params: TReviewRequestListParams): Promise<IReviewRequest[]> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-requests/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async submitReviewRequest(workspaceSlug: string, data: TReviewRequestCreatePayload): Promise<IReviewRequest> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-requests/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async getReviewRequest(workspaceSlug: string, requestId: string): Promise<IReviewRequestDetail> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-requests/${requestId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async withdrawReviewRequest(workspaceSlug: string, requestId: string): Promise<IReviewRequest> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-requests/${requestId}/withdraw/`, {})
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async getReviewSessions(workspaceSlug: string, params: TReviewSessionListParams): Promise<IReviewSession[]> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-sessions/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async createReviewSession(workspaceSlug: string, data: TReviewSessionCreatePayload): Promise<IReviewSession> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async getReviewSession(workspaceSlug: string, sessionId: string): Promise<IReviewSessionDetail> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async updateReviewSession(
    workspaceSlug: string,
    sessionId: string,
    data: TReviewSessionUpdatePayload
  ): Promise<IReviewSession> {
    return this.patch(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async completeReviewSession(workspaceSlug: string, sessionId: string): Promise<IReviewSession> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/complete/`, {})
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async cancelReviewSession(workspaceSlug: string, sessionId: string): Promise<IReviewSession> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/cancel/`, {})
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async getSessionItems(workspaceSlug: string, sessionId: string): Promise<IReviewSessionItem[]> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/items/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async addSessionItems(workspaceSlug: string, sessionId: string, requestIds: string[]): Promise<IReviewSessionItem[]> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/items/`, {
      request_ids: requestIds,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async updateSessionItem(
    workspaceSlug: string,
    sessionId: string,
    itemId: string,
    data: TReviewItemUpdatePayload
  ): Promise<IReviewSessionItem> {
    return this.patch(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/items/${itemId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async removeSessionItem(workspaceSlug: string, sessionId: string, itemId: string): Promise<void> {
    await this.delete(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/items/${itemId}/`).catch(
      (error) => {
        throw toReviewError(error);
      }
    );
  }

  async generateReviewBriefing(
    workspaceSlug: string,
    sessionId: string,
    language: string
  ): Promise<IReviewSessionBriefing> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/briefing/`, { language })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async getSessionParticipants(workspaceSlug: string, sessionId: string): Promise<IReviewParticipant[]> {
    return this.get(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/participants/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async addSessionParticipant(
    workspaceSlug: string,
    sessionId: string,
    data: TReviewParticipantCreatePayload
  ): Promise<IReviewParticipant> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/participants/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async updateSessionParticipant(
    workspaceSlug: string,
    sessionId: string,
    participantId: string,
    data: TReviewParticipantUpdatePayload
  ): Promise<IReviewParticipant> {
    return this.patch(
      `${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/participants/${participantId}/`,
      data
    )
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }

  async removeSessionParticipant(workspaceSlug: string, sessionId: string, participantId: string): Promise<void> {
    await this.delete(
      `${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/participants/${participantId}/`
    ).catch((error) => {
      throw toReviewError(error);
    });
  }
}
