/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// helpers
import { API_BASE_URL } from "@plane/constants";
import type { AI_EDITOR_TASKS } from "@plane/constants";
// services
import { APIService } from "@/services/api.service";
// types
import type { TAiConversation, TAiStoredMessage } from "@/lib/ai-conversations";
import type { TAiAgentPendingAction } from "@/lib/ai-schedule";
// FIXME:
// import { IGptResponse } from "@plane/types";
// helpers

export type TTaskPayload = {
  casual_score?: number;
  formal_score?: number;
  task: AI_EDITOR_TASKS;
  text_input: string;
};

export type TChatPayload = {
  task: string;
  prompt: string;
  context: string;
  conversation_id: string;
};

export type TChatResponse = {
  response: string;
  response_html?: string;
  tool_calls?: { name: string; arguments: unknown }[];
  pending_action?: TAiAgentPendingAction | null;
  conversation: TAiConversation;
  user_message: TAiStoredMessage;
  assistant_message: TAiStoredMessage;
};

export type TAgentTaskResponse = TChatResponse;

export type TCompleteTaskResponse = {
  response: string;
  response_html?: string;
};

export class AIService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async createGptTask(workspaceSlug: string, data: TChatPayload): Promise<TChatResponse> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-assistant/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async createAgentTask(workspaceSlug: string, data: TChatPayload): Promise<TChatResponse> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-agent/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async completeTask(workspaceSlug: string, data: { task: string; prompt: string }): Promise<TCompleteTaskResponse> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-complete/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async performEditorTask(
    workspaceSlug: string,
    data: TTaskPayload
  ): Promise<{
    response: string;
  }> {
    return this.post(`/api/workspaces/${workspaceSlug}/rephrase-grammar/`, data)
      .then((res) => res?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
}
