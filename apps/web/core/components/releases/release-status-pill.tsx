import { observer } from "mobx-react";
// plane imports
import { RELEASE_STATUS_CONFIG } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TReleaseStatus } from "@plane/types";
// helpers
import { cn } from "@plane/utils";

type Props = {
  status: TReleaseStatus;
  className?: string;
};

export const ReleaseStatusPill = observer(function ReleaseStatusPill({ status, className }: Props) {
  const { t } = useTranslation();
  const config = RELEASE_STATUS_CONFIG[status];

  return (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", config.pill, className)}>
      {t(config.label_key)}
    </span>
  );
});
