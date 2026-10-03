/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
// plane imports
import type { IService, IServiceIncident, TServiceHealth } from "@plane/types";
import { cn } from "@plane/utils";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { ServiceHealthDot } from "../health/service-health-dot";

export type TServiceNodeData = {
  service: IService;
  statusLabel: string;
  criticalityLabel: string;
  health: TServiceHealth;
  incidents: IServiceIncident[];
  /** Blast-radius map: dependency neighbors are dimmed. */
  dimmed?: boolean;
};

export function ServiceNode({ data }: NodeProps<Node<TServiceNodeData>>) {
  const service = (data as TServiceNodeData | undefined)?.service;
  if (!service) return null;
  const { statusLabel, criticalityLabel, health, incidents, dimmed } = data as TServiceNodeData;
  const state = health ?? DEFAULT_HEALTH;
  const config = HEALTH_CONFIG[state];

  return (
    <div className={cn("relative w-[220px]", dimmed && "opacity-60")}>
      <Handle type="target" position={Position.Left} />
      <div className="service-node-card shadow-sm relative overflow-hidden rounded-md border border-subtle bg-surface-1 px-3 py-2 transition-colors">
        <span aria-hidden="true" className={cn("absolute inset-y-0 left-0 w-[3px]", config.rail)} />
        <div className="flex min-w-0 items-center gap-2 pl-1">
          <ServiceHealthDot health={state} />
          <span className="text-sm min-w-0 flex-1 truncate font-medium text-primary" title={service.name}>
            {service.name}
          </span>
        </div>
        <div className="text-xs mt-1 pl-1 text-secondary capitalize">
          {statusLabel} · {criticalityLabel}
        </div>
      </div>
      {incidents.length > 0 && (
        <span className="absolute -top-1.5 -right-1.5 grid h-4 min-w-4 place-items-center rounded-full bg-danger-primary px-1 text-10 font-medium text-on-color">
          {incidents.length}
        </span>
      )}
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
