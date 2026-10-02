import { observer } from "mobx-react";
// plane imports
import { REVIEW_OUTCOME_CONFIG } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TReviewOutcome } from "@plane/types";
// helpers
import { cn } from "@plane/utils";

type Props = {
  outcome: TReviewOutcome;
  className?: string;
};

export const ReviewOutcomePill = observer(function ReviewOutcomePill({ outcome, className }: Props) {
  const { t } = useTranslation();
  const config = REVIEW_OUTCOME_CONFIG[outcome];

  return (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", config.pill, className)}>
      {t(config.label_key)}
    </span>
  );
});
