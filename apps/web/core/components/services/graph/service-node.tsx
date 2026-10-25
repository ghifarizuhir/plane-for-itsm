/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import { ClockOutline, LockOutline, WarningTriangleOutline } from "@makeplane/propel/icons";
// plane imports
import type { ILiteService, IService, IServiceIncident, TServiceCriticality, TServiceHealth } from "@plane/types";
import { calculateTimeAgo, calculateTimeAgoShort, cn } from "@plane/utils";
// helpers
import { getHighestSeverityIncident } from "@/services/service-health.helpers";
import { isLiteService } from "@/services/service.helpers";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { ServiceHealthDot } from "../health/service-health-dot";

export type TServiceNodeData = {
  service: IService | ILiteService;
  accessLevel?: "full" | "lite";
  statusLabel: string;
  criticalityLabel: string;
  typeLabel: string;
  healthLabel?: string;
  health: TServiceHealth;
  incidents: IServiceIncident[];
  lastDeployedAt?: string | null;
  highestSeverityLabel?: string;
  /** Blast-radius map: dependency neighbors are dimmed. */
  dimmed?: boolean;
  /** A node was selected elsewhere: everything outside its relationships fades further. */
  focusDimmed?: boolean;
  /** The selected node whose direct relationships are being traced. */
  focused?: boolean;
};

const CRITICALITY_BADGE_STYLE: Record<TServiceCriticality, string> = {
  critical: "border-danger-strong bg-danger-subtle text-danger-primary",
  high: "border-subtle bg-warning-subtle text-warning-primary",
  medium: "border-subtle bg-layer-2 text-secondary",
  low: "border-subtle bg-layer-2 text-tertiary",
};

const HEALTH_BORDER_STYLE: Record<TServiceHealth, string> = {
  down: "border-danger-strong",
  degraded: "border-subtle-1",
  healthy: "border-subtle",
  unknown: "border-subtle",
};

export function ServiceNode({ data }: NodeProps<Node<TServiceNodeData>>) {
  const service = (data as TServiceNodeData | undefined)?.service;
  if (!service) return null;
  const {
    statusLabel,
    criticalityLabel,
    typeLabel,
    healthLabel,
    health,
    incidents,
    lastDeployedAt,
    highestSeverityLabel,
    dimmed,
    focusDimmed,
    focused,
    accessLevel,
  } = data as TServiceNodeData;
  const state = health ?? DEFAULT_HEALTH;
  const config = HEALTH_CONFIG[state];
  const isLite = accessLevel === "lite" || isLiteService(service);
  const highestIncident = getHighestSeverityIncident(incidents);
  const hasIncidents = incidents.length > 0 && Boolean(highestIncident);
  const isSevereIncident = highestIncident?.severity === "sev1" || highestIncident?.severity === "sev2";
  const deployedAgo = lastDeployedAt ? calculateTimeAgo(lastDeployedAt) : "";
  const deployedAgoShort = lastDeployedAt ? calculateTimeAgoShort(lastDeployedAt) : "";

  return (
    <div
      id={`service-${service.id}`}
      className={cn(
        "group relative w-[256px] transition-opacity duration-150",
        dimmed && "opacity-55",
        focusDimmed && "opacity-30",
        isLite && "opacity-80"
      )}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!h-2.5 !w-2.5 !border-2 transition-transform duration-150 group-hover:scale-125"
      />
      <div
        className={cn(
          "service-node-card relative flex flex-col gap-2 overflow-hidden rounded-lg border bg-layer-1 p-3 shadow-raised-100 transition-colors duration-150 hover:bg-layer-1-hover",
          HEALTH_BORDER_STYLE[state],
          focused && "is-focused",
          isLite && "border-dashed"
        )}
        title={isLite ? `${service.name} — ${typeLabel}` : service.name}
      >
        {/* Top telemetry & identity row */}
        <div className="flex items-center justify-between gap-2">
          <span
            className={cn(
              "inline-flex min-w-0 items-center gap-1 rounded-xs border border-subtle bg-layer-2 px-1.5 py-0.5 text-10 font-medium text-secondary",
              isLite && "text-tertiary"
            )}
            title={typeLabel}
          >
            {isLite && <LockOutline className="h-3 w-3 shrink-0 text-tertiary" />}
            <span className="truncate">{typeLabel}</span>
          </span>

          <span
            className={cn(
              "inline-flex shrink-0 items-center gap-1.5 rounded-full px-2 py-0.5 text-10 font-medium",
              config.pill
            )}
            title={healthLabel}
          >
            <ServiceHealthDot health={state} className={cn("h-1.5 w-1.5", state === "down" && "animate-pulse")} />
            {healthLabel && <span className="leading-none">{healthLabel}</span>}
          </span>
        </div>

        {/* Service primary title */}
        <div className="flex min-w-0 items-center gap-1.5">
          <span
            className={cn(
              "min-w-0 flex-1 truncate text-13 font-semibold tracking-[-0.01em]",
              isLite ? "text-tertiary" : "text-primary"
            )}
            title={service.name}
          >
            {service.name}
          </span>
        </div>

        {/* Operational footer strip */}
        <div className="flex items-center justify-between gap-2 border-t border-subtle pt-2">
          <div className="flex min-w-0 items-center gap-1.5">
            <span
              className={cn(
                "shrink-0 rounded-xs border px-1.5 py-0.5 font-code text-9 font-medium tracking-wide uppercase",
                CRITICALITY_BADGE_STYLE[service.criticality] ?? CRITICALITY_BADGE_STYLE.medium
              )}
            >
              {criticalityLabel}
            </span>
            <span className="truncate text-11 text-secondary capitalize">{statusLabel}</span>
          </div>

          {hasIncidents ? (
            <span
              className={cn(
                "inline-flex shrink-0 items-center gap-1 rounded-xs px-1.5 py-0.5 font-code text-10 font-medium tabular-nums",
                isSevereIncident ? "bg-danger-subtle text-danger-primary" : "bg-warning-subtle text-warning-primary"
              )}
            >
              <WarningTriangleOutline className="h-3 w-3 shrink-0" />
              <span>{incidents.length}</span>
              {highestSeverityLabel && (
                <>
                  <span className="opacity-60">·</span>
                  <span>{highestSeverityLabel}</span>
                </>
              )}
            </span>
          ) : (
            lastDeployedAt && (
              <span
                className="inline-flex shrink-0 items-center gap-1 font-code text-10 text-secondary tabular-nums"
                title={deployedAgo}
                aria-label={deployedAgo}
              >
                <ClockOutline className="h-3 w-3 shrink-0 text-tertiary" />
                <span>{deployedAgoShort}</span>
              </span>
            )
          )}
        </div>
      </div>
      <Handle
        type="source"
        position={Position.Right}
        className="!h-2.5 !w-2.5 !border-2 transition-transform duration-150 group-hover:scale-125"
      />
    </div>
  );
}
