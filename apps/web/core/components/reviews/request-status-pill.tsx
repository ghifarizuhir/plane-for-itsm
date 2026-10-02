import { observer } from "mobx-react";
// plane imports
import { REVIEW_REQUEST_STATUS_CONFIG } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TReviewRequestStatus } from "@plane/types";
// helpers
import { cn } from "@plane/utils";

type Props = {
  status: TReviewRequestStatus;
  className?: string;
};

export const ReviewRequestStatusPill = observer(function ReviewRequestStatusPill({ status, className }: Props) {
  const { t } = useTranslation();
  const config = REVIEW_REQUEST_STATUS_CONFIG[status];

  return (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", config.pill, className)}>
      {t(config.label_key)}
    </span>
  );
});
