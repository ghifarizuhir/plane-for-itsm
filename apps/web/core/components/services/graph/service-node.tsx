/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import type { IService, TServiceCriticality } from "@plane/types";

export type TServiceNodeData = {
  service: IService;
};

const CRITICALITY_CLASS: Record<TServiceCriticality, string> = {
  critical: "bg-red-500",
  high: "bg-orange-500",
  medium: "bg-yellow-500",
  low: "bg-green-500",
};

export function ServiceNode({ data }: NodeProps<Node<TServiceNodeData>>) {
  const service = (data as TServiceNodeData | undefined)?.service;
  if (!service) return null;
  return (
    <div className="shadow-sm w-[220px] rounded-md border border-subtle bg-surface-1 px-3 py-2">
      <Handle type="target" position={Position.Left} className="!bg-surface-2" />
      <div className="flex min-w-0 items-center gap-2">
        <span
          aria-hidden="true"
          className={`h-2 w-2 shrink-0 rounded-full ${CRITICALITY_CLASS[service.criticality] ?? "bg-gray-400"}`}
        />
        <span className="text-sm min-w-0 flex-1 truncate font-medium text-primary" title={service.name}>
          {service.name}
        </span>
      </div>
      <div className="text-xs mt-1 text-secondary capitalize">
        {service.status} · {service.criticality}
      </div>
      <Handle type="source" position={Position.Right} className="!bg-surface-2" />
    </div>
  );
}
