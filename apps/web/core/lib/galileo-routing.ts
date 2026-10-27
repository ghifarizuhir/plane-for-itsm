/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const buildGalileoHref = (workspaceSlug: string): string => `/${workspaceSlug}/galileo`;

export const buildGalileoConversationHref = (workspaceSlug: string, conversationId: string): string =>
  `/${workspaceSlug}/galileo/${conversationId}`;

export const canUseGalileo = (hasLlmConfigured: boolean, canUseAi: boolean): boolean => hasLlmConfigured && canUseAi;

export const shouldNavigateToNewConversation = (
  routeConversationId: string | undefined,
  activeConversationId: string | undefined
): boolean => !routeConversationId && !!activeConversationId;
