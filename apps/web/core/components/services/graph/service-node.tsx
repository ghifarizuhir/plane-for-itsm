/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import { motion, useReducedMotion } from "framer-motion";
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
import { SERVICE_KIND_ICON } from "../kind-config";

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
  noDeploymentLabel: string;
  highestSeverityLabel?: string;
  /** Blast-radius map: dependency neighbors are dimmed. */
  dimmed?: boolean;
  /** A node was selected elsewhere: everything outside its relationships fades further. */
  focusDimmed?: boolean;
  /** The selected node whose direct relationships are being traced. */
  focused?: boolean;
};

const CRITICALITY_TEXT_STYLE: Record<TServiceCriticality, string> = {
  critical: "text-danger-primary",
  high: "text-warning-primary",
  medium: "text-secondary",
  low: "text-tertiary",
};

const HEALTH_BORDER_STYLE: Record<TServiceHealth, string> = {
  down: "border-danger-strong",
  degraded: "border-subtle-1",
  healthy: "border-subtle",
  unknown: "border-subtle",
};

export function ServiceNode({ data }: NodeProps<Node<TServiceNodeData>>) {
  const reducedMotion = useReducedMotion();
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
    noDeploymentLabel,
    highestSeverityLabel,
    dimmed,
    focusDimmed,
    focused,
    accessLevel,
  } = data as TServiceNodeData;
  const state = health ?? DEFAULT_HEALTH;
  const config = HEALTH_CONFIG[state];
  const isLite = accessLevel === "lite" || isLiteService(service);
  const kind = !isLite && "kind" in service ? service.kind : "other";
  const KindIcon = isLite ? LockOutline : SERVICE_KIND_ICON[kind];
  const kindAccent = `var(--kind-${kind.replace(/_/g, "-")})`;
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
      <motion.div
        className={cn(
          "service-node-card relative flex flex-col gap-2 overflow-hidden rounded-lg border bg-layer-1 p-3 shadow-raised-100 transition-[background-color,border-color,box-shadow] duration-150 hover:bg-layer-1-hover hover:shadow-raised-200",
          HEALTH_BORDER_STYLE[state],
          focused && "is-focused",
          isLite && "border-dashed"
        )}
        whileHover={reducedMotion ? undefined : { y: -2 }}
        whileTap={reducedMotion ? undefined : { scale: 0.995 }}
        transition={{ type: "spring", stiffness: 420, damping: 28 }}
        title={isLite ? `${service.name} — ${typeLabel}` : service.name}
      >
        <span aria-hidden className={cn("absolute inset-y-2.5 left-0 w-[3px] rounded-r-full", config.rail)} />

        <div className="flex min-w-0 items-center gap-1">
          <span
            className={cn("min-w-0 truncate text-10 font-medium", isLite ? "text-tertiary" : "text-secondary")}
            title={typeLabel}
          >
            {typeLabel}
          </span>
          <span aria-hidden className="shrink-0 text-9 text-placeholder">
            ·
          </span>
          <span
            className={cn(
              "inline-flex shrink-0 items-center gap-1 font-code text-9 font-semibold tracking-[0.08em] uppercase",
              CRITICALITY_TEXT_STYLE[service.criticality] ?? CRITICALITY_TEXT_STYLE.medium
            )}
          >
            <span aria-hidden className="h-1 w-1 rounded-[1px] bg-current" />
            {criticalityLabel}
          </span>
          <span aria-hidden className="shrink-0 text-9 text-placeholder">
            ·
          </span>
          <span className="shrink-0 truncate text-10 text-secondary capitalize">{statusLabel}</span>

          <span
            className={cn(
              "ml-auto inline-flex shrink-0 items-center gap-1 rounded-full px-1.5 py-0.5 text-9 font-semibold",
              config.pill
            )}
            title={healthLabel}
          >
            <ServiceHealthDot health={state} className={cn("h-1.5 w-1.5", state === "down" && "animate-pulse")} />
            {healthLabel && <span className="leading-none">{healthLabel}</span>}
          </span>
        </div>

        <div className="flex min-w-0 items-center gap-1.5">
          <span className="grid h-4 w-4 shrink-0 place-items-center" style={{ color: kindAccent }} title={typeLabel}>
            <KindIcon className="h-3.5 w-3.5" />
          </span>
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

        <div className="flex items-center justify-between gap-2 border-t border-subtle pt-2">
          <span
            className={cn(
              "inline-flex min-w-0 items-center gap-1 font-code text-10 tabular-nums",
              lastDeployedAt ? "text-secondary" : "text-tertiary"
            )}
            title={lastDeployedAt ? deployedAgo : noDeploymentLabel}
            aria-label={lastDeployedAt ? deployedAgo : noDeploymentLabel}
          >
            <ClockOutline className="h-3 w-3 shrink-0 text-tertiary" />
            <span className="truncate">{lastDeployedAt ? deployedAgoShort : noDeploymentLabel}</span>
          </span>

          {hasIncidents && (
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
          )}
        </div>
      </motion.div>
      <Handle
        type="source"
        position={Position.Right}
        className="!h-2.5 !w-2.5 !border-2 transition-transform duration-150 group-hover:scale-125"
      />
    </div>
  );
}
