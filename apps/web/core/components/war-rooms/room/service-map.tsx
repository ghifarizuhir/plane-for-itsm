/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService, IServiceHealthSnapshot, IWarRoom, TServiceGraphData } from "@plane/types";
// components
import { ServiceGraphCanvas } from "@/components/services/graph/service-graph-canvas";
import { ServiceMultiSelect } from "@/components/services/select/service-multi-select";
// helpers
import { getBlastRadius } from "@/services/war-room.helpers";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
};

export const WarRoomServiceMap = observer(function WarRoomServiceMap({
  workspaceSlug,
  projectId,
  room,
  canWrite,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { fetchedMap, loader, fetchServices, getServiceById, getServiceHealth, getDependenciesByProject } =
    useService();
  const { addServices, removeService } = useWarRoom();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const workspaceId = currentWorkspace?.id;
  const affectedServiceIds = useMemo(() => room.services.map((service) => service.id), [room.services]);
  const dependencies = getDependenciesByProject(projectId);
  const blastRadius = useMemo(
    () => getBlastRadius(affectedServiceIds, dependencies),
    [affectedServiceIds, dependencies]
  );
  const graphData: TServiceGraphData = useMemo(() => {
    const includedIds = new Set(blastRadius.includedIds);
    const services = blastRadius.includedIds
      .map((serviceId) => getServiceById(serviceId))
      .filter((service): service is IService => Boolean(service));
    const health: Record<string, IServiceHealthSnapshot> = {};
    services.forEach((service) => {
      const snapshot = getServiceHealth(service.id);
      if (snapshot) health[service.id] = snapshot;
    });
    return {
      services,
      dependencies: dependencies.filter(
        (dependency) => includedIds.has(dependency.from_service_id) && includedIds.has(dependency.to_service_id)
      ),
      health,
    };
  }, [blastRadius, dependencies, getServiceById, getServiceHealth]);

  useEffect(() => {
    if (!workspaceId || fetchedMap[projectId] || loader) return;
    void fetchServices(workspaceSlug, workspaceId, projectId);
  }, [fetchedMap, fetchServices, loader, projectId, workspaceId, workspaceSlug]);

  const handleServicesChange = async (nextIds: string[]) => {
    const current = new Set(affectedServiceIds);
    const next = new Set(nextIds);
    const toAdd = nextIds.filter((serviceId) => !current.has(serviceId));
    const toRemove = affectedServiceIds.filter((serviceId) => !next.has(serviceId));
    try {
      if (toAdd.length > 0) await addServices(workspaceSlug, projectId, room.id, toAdd);
      await Promise.all(toRemove.map((serviceId) => removeService(workspaceSlug, projectId, room.id, serviceId)));
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("toast.error"),
        message: t("war_room.map.link_failed"),
      });
    }
  };

  const handleNodeClick = (serviceId: string) => {
    window.open(`/${workspaceSlug}/projects/${projectId}/services/${serviceId}`, "_blank", "noopener,noreferrer");
  };

  return (
    <div className="flex h-full w-full flex-col bg-surface-1">
      <div className="flex shrink-0 flex-col gap-2 border-b border-subtle px-3 py-2">
        <div className="flex items-center justify-between gap-2">
          <span className="font-code text-10 font-medium tracking-[0.14em] text-tertiary uppercase">
            {t("war_room.map.title")}
          </span>
          <span className="font-code text-10 text-secondary">
            {affectedServiceIds.length} {t("war_room.map.affected_legend")}
          </span>
        </div>
        <div className="flex items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2">
            <span className="flex items-center gap-1 text-10 text-secondary">
              <span className="size-1.5 rounded-full bg-danger-primary" />
              {t("war_room.map.affected_legend")}
            </span>
            <span className="flex items-center gap-1 text-10 text-tertiary">
              <span className="size-1.5 rounded-full bg-layer-3" />
              {t("war_room.map.neighbor_legend")}
            </span>
          </div>
          {canWrite && (
            <div className="shrink-0">
              <ServiceMultiSelect
                workspaceSlug={workspaceSlug}
                projectId={projectId}
                value={affectedServiceIds}
                onChange={(serviceIds) => void handleServicesChange(serviceIds)}
              />
            </div>
          )}
        </div>
      </div>
      <div className="min-h-0 flex-1">
        {graphData.services.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
            <p className="text-13 font-medium text-primary">{t("war_room.map.empty_title")}</p>
            <p className="text-12 text-secondary">{t("war_room.map.empty_description")}</p>
          </div>
        ) : (
          <ServiceGraphCanvas
            graphData={graphData}
            readOnly
            highlightedServiceIds={blastRadius.affectedIds}
            onNodeClick={handleNodeClick}
          />
        )}
      </div>
    </div>
  );
});
