/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import Link from "next/link";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { cn } from "@plane/utils";
// components
import { ButtonAvatars } from "@/components/dropdowns/member/avatar";
// hooks
import { useService } from "@/hooks/store/use-service";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { ServiceDeployCell } from "../health/service-deploy-cell";
import { ServiceHealthDot } from "../health/service-health-dot";
import { ServiceHealthPill } from "../health/service-health-pill";
import { ServiceIncidentCell } from "../health/service-incident-cell";

type Props = {
  serviceId: string;
};

export const ServicesBoardRow = observer(function ServicesBoardRow(props: Props) {
  const { serviceId } = props;
  // router
  const { workspaceSlug } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, getServiceHealth } = useService();
  // derived values
  const service = getServiceById(serviceId);
  const health = getServiceHealth(serviceId);

  if (!service) return null;

  const state = health?.health ?? DEFAULT_HEALTH;
  const config = HEALTH_CONFIG[state];
  const serviceLink = `/${workspaceSlug?.toString()}/projects/${service.project_id}/services/${service.id}`;
  const isCritical = service.criticality === "critical";

  return (
    <Link
      href={serviceLink}
      className="relative flex items-center gap-3 border-b border-subtle px-3 py-2.5 transition-colors hover:bg-layer-transparent-hover"
    >
      <span aria-hidden="true" className={cn("absolute inset-y-0 left-0 w-[3px]", config.rail)} />
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <ServiceHealthDot health={state} />
        <span className="truncate text-13 font-medium text-primary">{service.name}</span>
        <span
          className={cn(
            "shrink-0 rounded-xs border px-1 text-10 font-medium tracking-wide uppercase",
            isCritical ? "border-danger-strong text-danger-primary" : "border-subtle text-tertiary"
          )}
        >
          {t(`service.criticality_values.${service.criticality}`)}
        </span>
      </div>
      <div className="hidden w-[120px] shrink-0 sm:block">
        <ServiceHealthPill health={state} />
      </div>
      <div className="hidden w-[120px] shrink-0 md:block">
        <ServiceIncidentCell incidents={health?.incidents ?? []} />
      </div>
      <div className="hidden w-[110px] shrink-0 lg:block">
        <ServiceDeployCell lastDeployedAt={health?.last_deployed_at ?? null} />
      </div>
      <div className="hidden w-[56px] shrink-0 items-center justify-end xl:flex">
        {service.owner_id && <ButtonAvatars showTooltip userIds={service.owner_id} />}
      </div>
    </Link>
  );
});
