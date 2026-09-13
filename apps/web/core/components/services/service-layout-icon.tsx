/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { GridOutline, ListOutline, WorkgraphOutline } from "@makeplane/propel/icons";
import type { TServiceLayoutOptions } from "@plane/types";
import { cn } from "@plane/utils";

export const SERVICE_VIEW_LAYOUTS: { key: TServiceLayoutOptions; i18n_label: string }[] = [
  { key: "list", i18n_label: "service.layout.list" },
  { key: "grid", i18n_label: "service.layout.grid" },
  { key: "graph", i18n_label: "service.layout.graph" },
];

interface IServiceLayoutIcon {
  className?: string;
  containerClassName?: string;
  layoutType: TServiceLayoutOptions;
  size?: number;
  withContainer?: boolean;
}

export function ServiceLayoutIcon(props: IServiceLayoutIcon) {
  const { layoutType, className = "", containerClassName = "", size = 14, withContainer = false } = props;

  // get Layout icon
  const icons = {
    list: ListOutline,
    grid: GridOutline,
    graph: WorkgraphOutline,
  };
  const Icon = icons[layoutType ?? "list"];

  if (!Icon) return null;

  return (
    <>
      {withContainer ? (
        <div
          className={cn("flex flex-shrink-0 items-center justify-center rounded-sm border p-0.5", containerClassName)}
        >
          <Icon width={size} height={size} className={cn(className)} />
        </div>
      ) : (
        <Icon width={size} height={size} className={cn("flex-shrink-0", className)} />
      )}
    </>
  );
}
