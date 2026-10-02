/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TNotificationData, TNotificationReviewRequest, TNotificationReviewSession } from "@plane/types";

export const isReviewRequestNotification = (
  data: TNotificationData | undefined
): data is TNotificationData & { review_request: TNotificationReviewRequest } =>
  typeof data?.review_request?.id === "string" && data.review_request.id.length > 0;

export const isReviewSessionNotification = (
  data: TNotificationData | undefined
): data is TNotificationData & { review_session: TNotificationReviewSession } =>
  typeof data?.review_session?.id === "string" && data.review_session.id.length > 0;
