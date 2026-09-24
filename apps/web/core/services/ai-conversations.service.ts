/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import { APIService } from "@/services/api.service";
import type {
  TAiConversation,
  TAiConversationMode,
  TAiMessageMetadataPatch,
  TAiStoredMessage,
} from "@/lib/ai-conversations";

export class AiConversationsService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async list(workspaceSlug: string): Promise<TAiConversation[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/ai-conversations/`)
      .then((response) => response?.data?.conversations ?? [])
      .catch((error) => {
        throw error?.response;
      });
  }

  async create(workspaceSlug: string, mode: TAiConversationMode, title?: string): Promise<TAiConversation> {
    return this.post(`/api/workspaces/${workspaceSlug}/ai-conversations/`, { mode, title })
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async retrieve(workspaceSlug: string, conversationId: string): Promise<TAiConversation> {
    return this.get(`/api/workspaces/${workspaceSlug}/ai-conversations/${conversationId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async update(workspaceSlug: string, conversationId: string, title: string): Promise<TAiConversation> {
    return this.patch(`/api/workspaces/${workspaceSlug}/ai-conversations/${conversationId}/`, { title })
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }

  async remove(workspaceSlug: string, conversationId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/ai-conversations/${conversationId}/`)
      .then(() => undefined)
      .catch((error) => {
        throw error?.response;
      });
  }

  async listMessages(workspaceSlug: string, conversationId: string): Promise<TAiStoredMessage[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/ai-conversations/${conversationId}/messages/`)
      .then((response) => response?.data?.messages ?? [])
      .catch((error) => {
        throw error?.response;
      });
  }

  async updateMessageMetadata(
    workspaceSlug: string,
    conversationId: string,
    messageId: string,
    metadata: TAiMessageMetadataPatch
  ): Promise<TAiStoredMessage> {
    return this.patch(`/api/workspaces/${workspaceSlug}/ai-conversations/${conversationId}/messages/${messageId}/`, {
      metadata,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response;
      });
  }
}
