import { observer } from "mobx-react";
// plane imports
import { REVIEW_SESSION_STATUS_CONFIG } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TReviewSessionStatus } from "@plane/types";
import { cn } from "@plane/utils";

type Props = {
  status: TReviewSessionStatus;
  className?: string;
};

export const ReviewSessionStatusPill = observer(function ReviewSessionStatusPill({ status, className }: Props) {
  const { t } = useTranslation();
  const config = REVIEW_SESSION_STATUS_CONFIG[status];

  return (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", config.pill, className)}>
      {t(config.label_key)}
    </span>
  );
});
