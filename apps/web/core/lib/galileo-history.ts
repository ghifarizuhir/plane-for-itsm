/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TAiConversation } from "@/lib/ai-conversations";

export type TGallileoHistoryGroup = "today" | "yesterday" | "previous7Days" | "older";

export type TGallileoHistoryGroups = Record<TGallileoHistoryGroup, TAiConversation[]>;

const startOfDay = (date: Date): Date => new Date(date.getFullYear(), date.getMonth(), date.getDate());

export const groupConversationsByDate = (
  conversations: TAiConversation[],
  now: Date = new Date()
): TGallileoHistoryGroups => {
  const today = startOfDay(now);
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  const previous7Days = new Date(today);
  previous7Days.setDate(today.getDate() - 7);

  const groups: TGallileoHistoryGroups = {
    today: [],
    yesterday: [],
    previous7Days: [],
    older: [],
  };

  const sorted = [...conversations].toSorted((a, b) => {
    const aTime = new Date(a.updated_at).getTime();
    const bTime = new Date(b.updated_at).getTime();
    const safeA = Number.isNaN(aTime) ? Number.NEGATIVE_INFINITY : aTime;
    const safeB = Number.isNaN(bTime) ? Number.NEGATIVE_INFINITY : bTime;
    return safeB - safeA;
  });

  for (const conversation of sorted) {
    const updatedAt = new Date(conversation.updated_at);
    if (Number.isNaN(updatedAt.getTime())) {
      groups.older.push(conversation);
    } else if (updatedAt >= today) {
      groups.today.push(conversation);
    } else if (updatedAt >= yesterday) {
      groups.yesterday.push(conversation);
    } else if (updatedAt >= previous7Days) {
      groups.previous7Days.push(conversation);
    } else {
      groups.older.push(conversation);
    }
  }

  return groups;
};
