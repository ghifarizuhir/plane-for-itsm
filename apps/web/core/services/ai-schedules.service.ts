import { API_BASE_URL } from "@plane/constants";
import { APIService } from "@/services/api.service";
import type { TAiSchedule, TAiScheduleProposal, TAiScheduleRun } from "@/lib/ai-schedule";

export class AiSchedulesService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async list(workspaceSlug: string): Promise<TAiSchedule[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/ai-schedules/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async create(
    workspaceSlug: string,
    proposal: TAiScheduleProposal,
    proposalKey: string
  ): Promise<{ id: string; already_exists?: boolean }> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-schedules/`, { ...proposal, proposal_key: proposalKey })
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async retrieve(workspaceSlug: string, scheduleId: string): Promise<TAiSchedule & { runs: TAiScheduleRun[] }> {
    return this.get(`/api/workspaces/${workspaceSlug}/ai-schedules/${scheduleId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async update(workspaceSlug: string, scheduleId: string, enabled: boolean): Promise<void> {
    return this.patch(`/api/workspaces/${workspaceSlug}/ai-schedules/${scheduleId}/`, { enabled })
      .then(() => undefined)
      .catch((error) => {
        throw error?.response;
      });
  }

  async remove(workspaceSlug: string, scheduleId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/ai-schedules/${scheduleId}/`)
      .then(() => undefined)
      .catch((error) => {
        throw error?.response;
      });
  }

  async runNow(workspaceSlug: string, scheduleId: string): Promise<{ run_id: string }> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-schedules/${scheduleId}/run/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }
}
