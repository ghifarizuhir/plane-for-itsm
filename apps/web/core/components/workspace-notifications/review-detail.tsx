/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useRouter } from "next/navigation";
// plane imports
import {
  getReleaseControlLink,
  getReviewSessionLink,
  getTestingControlLink,
  REVIEW_BOARD_LABEL_KEYS,
} from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { TNotificationReviewRequest, TNotificationReviewSession } from "@plane/types";

type Props = {
  workspaceSlug: string;
  reviewRequest?: TNotificationReviewRequest;
  reviewSession?: TNotificationReviewSession;
  embedRemoveCurrentNotification: () => void;
};

export function ReviewInboxDetail({
  workspaceSlug,
  reviewRequest,
  reviewSession,
  embedRemoveCurrentNotification,
}: Props) {
  const router = useRouter();
  const { t } = useTranslation();

  const boardType = reviewRequest?.board_type ?? reviewSession?.board_type ?? "tcb";
  const projectId = reviewRequest?.project_id ?? reviewSession?.project_id ?? undefined;
  const sessionId = reviewRequest?.session_id ?? reviewSession?.id;

  const open = () => {
    if (sessionId) {
      router.push(getReviewSessionLink(workspaceSlug, sessionId, boardType, projectId ?? undefined));
      return;
    }
    router.push(
      boardType === "rcb" ? getReleaseControlLink(workspaceSlug) : getTestingControlLink(workspaceSlug, projectId ?? "")
    );
  };

  return (
    <div className="h-full w-full overflow-y-auto p-4">
      <div className="mx-auto max-w-2xl space-y-3">
        <h3 className="text-base font-semibold break-words text-primary">
          {reviewRequest?.subject_label ?? reviewSession?.title ?? ""}
        </h3>
        <p className="text-xs text-tertiary">{t(REVIEW_BOARD_LABEL_KEYS[boardType])}</p>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" variant="secondary" onClick={open}>
            {t("review.notification.open")}
          </Button>
          <Button size="sm" variant="ghost" onClick={embedRemoveCurrentNotification}>
            {t("review.notification.dismiss")}
          </Button>
        </div>
      </div>
    </div>
  );
}
