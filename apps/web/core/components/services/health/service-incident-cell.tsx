/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { WarningTriangleOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { useTranslation } from "@plane/i18n";
import type { IServiceIncident } from "@plane/types";
import { cn } from "@plane/utils";
// helpers
import { getHighestSeverityIncident, getIncidentTooltipWindow } from "@/services/service-health.helpers";

type Props = {
  incidents: IServiceIncident[];
};

export function ServiceIncidentCell({ incidents }: Props) {
  const { t } = useTranslation();
  const highest = getHighestSeverityIncident(incidents);
  if (incidents.length === 0 || !highest) {
    return <span className="text-12 text-tertiary">—</span>;
  }
  const { visible, remaining } = getIncidentTooltipWindow(incidents);
  const tooltipLabel = [
    t("service.incidents.tooltip_title"),
    ...visible.map((incident) => {
      const parts = [incident.identifier, incident.name].filter((part): part is string => Boolean(part));
      if (incident.war_room_sequence_id) parts.push(`WR-${incident.war_room_sequence_id}`);
      parts.push(t(`service.incident_severity.${incident.severity}`));
      return parts.join(" · ");
    }),
    ...(remaining > 0 ? [t("service.incidents.more", { count: remaining })] : []),
  ].join(" · ");
  const isSevere = highest.severity === "sev1" || highest.severity === "sev2";
  return (
    <Tooltip label={tooltipLabel} layout="stacked">
      <span
        tabIndex={0}
        aria-label={tooltipLabel}
        className={cn(
          "flex items-center gap-1 text-12 font-medium focus-visible:outline-none",
          isSevere ? "text-danger-primary" : "text-warning-primary"
        )}
      >
        <WarningTriangleOutline className="h-3.5 w-3.5" />
        <span className="font-code tabular-nums">{incidents.length}</span>
        <span className="text-tertiary">·</span>
        <span>{t(`service.incident_severity.${highest.severity}`)}</span>
      </span>
    </Tooltip>
  );
}
