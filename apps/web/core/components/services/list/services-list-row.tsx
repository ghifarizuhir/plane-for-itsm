/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { ControlLink } from "@plane/ui";
import { cn } from "@plane/utils";
// components
import { ButtonAvatars } from "@/components/dropdowns/member/avatar";
// hooks
import { usePlatformOS } from "@/hooks/use-platform-os";
import useServicePeekOverviewRedirection from "@/hooks/use-service-peek-overview-redirection";
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { ServiceCriticalityPill } from "../health/service-criticality-pill";
import { ServiceDeployCell } from "../health/service-deploy-cell";
import { ServiceHealthDot } from "../health/service-health-dot";
import { ServiceHealthPill } from "../health/service-health-pill";
import { ServiceIncidentCell } from "../health/service-incident-cell";
import { ServiceStatusPill } from "../health/service-status-pill";

type Props = {
  serviceId: string;
};

export const ServicesListRow = observer(function ServicesListRow(props: Props) {
  const { serviceId } = props;
  // router
  const { workspaceSlug } = useParams();
  // store hooks
  const { getServiceById, getServiceHealth } = useService();
  const { getProjectById } = useProject();
  // peek overview
  const { isMobile } = usePlatformOS();
  const { handleRedirection } = useServicePeekOverviewRedirection();
  // derived values
  const service = getServiceById(serviceId);
  const health = getServiceHealth(serviceId);

  if (!service) return null;

  const state = health?.health ?? DEFAULT_HEALTH;
  const serviceLink = `/${workspaceSlug?.toString()}/services/${service.id}`;
  const ownerProject = service.owner_project_id ? getProjectById(service.owner_project_id) : undefined;

  return (
    <ControlLink
      id={`service-${service.id}`}
      href={serviceLink}
      onClick={() => handleRedirection(workspaceSlug?.toString(), service, isMobile)}
      className="relative flex items-center gap-3 border-b border-subtle px-3 py-1.5 transition-colors hover:bg-layer-transparent-hover"
    >
      <span aria-hidden="true" className={cn("absolute inset-y-0 left-0 w-[3px]", HEALTH_CONFIG[state].rail)} />
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <ServiceHealthDot health={state} />
        <span className="truncate text-12 font-medium text-primary">{service.name}</span>
        {ownerProject && (
          <span
            className="shrink-0 rounded-xs border border-subtle px-1 text-9 font-medium tracking-wide text-tertiary"
            title={ownerProject.name}
          >
            {ownerProject.identifier}
          </span>
        )}
      </div>
      <div className="hidden w-[100px] shrink-0 sm:block">
        <ServiceStatusPill status={service.status} />
      </div>
      <div className="hidden w-[90px] shrink-0 md:block">
        <ServiceCriticalityPill criticality={service.criticality} />
      </div>
      <div className="hidden w-[90px] shrink-0 md:block">
        <ServiceHealthPill health={state} />
      </div>
      <div className="hidden w-[100px] shrink-0 lg:block">
        <ServiceIncidentCell incidents={health?.incidents ?? []} />
      </div>
      <div className="hidden w-[90px] shrink-0 xl:block">
        <ServiceDeployCell lastDeployedAt={health?.last_deployed_at ?? null} />
      </div>
      <div className="hidden w-[40px] shrink-0 items-center justify-end xl:flex">
        {service.owner_id && <ButtonAvatars showTooltip userIds={service.owner_id} />}
      </div>
    </ControlLink>
  );
});
