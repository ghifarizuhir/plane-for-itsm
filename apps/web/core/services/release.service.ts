import { API_BASE_URL } from "@plane/constants";
import type {
  IRelease,
  IReleaseChange,
  IReleaseDetail,
  TReleaseCreatePayload,
  TReleaseUpdatePayload,
} from "@plane/types";
// services
import { APIService } from "@/services/api.service";

type TReleaseErrorBody = { detail?: string; error?: string; [key: string]: unknown };

const toReleaseError = (error: unknown): Error => {
  const body = (error as { response?: { data?: TReleaseErrorBody } })?.response?.data;
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

export class ReleaseService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  private basePath(workspaceSlug: string): string {
    return `/api/workspaces/${workspaceSlug}/releases`;
  }

  async getReleases(
    workspaceSlug: string,
    params?: { status?: string; target_date_from?: string; target_date_to?: string }
  ): Promise<IRelease[]> {
    return this.get(`${this.basePath(workspaceSlug)}/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async getRelease(workspaceSlug: string, releaseId: string): Promise<IReleaseDetail> {
    return this.get(`${this.basePath(workspaceSlug)}/${releaseId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async createRelease(workspaceSlug: string, data: TReleaseCreatePayload): Promise<IRelease> {
    return this.post(`${this.basePath(workspaceSlug)}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async updateRelease(workspaceSlug: string, releaseId: string, data: TReleaseUpdatePayload): Promise<IRelease> {
    return this.patch(`${this.basePath(workspaceSlug)}/${releaseId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async deleteRelease(workspaceSlug: string, releaseId: string): Promise<void> {
    await this.delete(`${this.basePath(workspaceSlug)}/${releaseId}/`).catch((error) => {
      throw toReleaseError(error);
    });
  }

  async getReleaseChanges(workspaceSlug: string, releaseId: string): Promise<IReleaseChange[]> {
    return this.get(`${this.basePath(workspaceSlug)}/${releaseId}/changes/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async linkReleaseChanges(workspaceSlug: string, releaseId: string, issueIds: string[]): Promise<IReleaseChange[]> {
    return this.post(`${this.basePath(workspaceSlug)}/${releaseId}/changes/`, { issue_ids: issueIds })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReleaseError(error);
      });
  }

  async unlinkReleaseChange(workspaceSlug: string, releaseId: string, issueId: string): Promise<void> {
    await this.delete(`${this.basePath(workspaceSlug)}/${releaseId}/changes/${issueId}/`).catch((error) => {
      throw toReleaseError(error);
    });
  }
}
