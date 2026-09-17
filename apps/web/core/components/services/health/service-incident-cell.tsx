/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { WarningTriangleOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import type { IServiceIncident } from "@plane/types";
import { cn } from "@plane/utils";
// helpers
import { getHighestSeverityIncident } from "@/services/service-health.helpers";

type Props = {
  incidents: IServiceIncident[];
};

export function ServiceIncidentCell({ incidents }: Props) {
  const { t } = useTranslation();
  const highest = getHighestSeverityIncident(incidents);
  if (incidents.length === 0 || !highest) {
    return <span className="text-12 text-tertiary">—</span>;
  }
  const isSevere = highest.severity === "sev1" || highest.severity === "sev2";
  return (
    <span
      className={cn(
        "flex items-center gap-1 text-12 font-medium",
        isSevere ? "text-danger-primary" : "text-warning-primary"
      )}
    >
      <WarningTriangleOutline className="h-3.5 w-3.5" />
      <span className="font-code tabular-nums">{incidents.length}</span>
      <span className="text-tertiary">·</span>
      <span>{t(`service.incident_severity.${highest.severity}`)}</span>
    </span>
  );
}
