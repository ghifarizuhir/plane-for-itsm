/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ComponentType } from "react";
import {
  ArchiveOutline,
  BoxesOutline,
  CacheOutline,
  ConnectionsOutline,
  CubeOutline,
  GlobeOutline,
  LibraryOutline,
  ListOutline,
  ServerOutline,
} from "@makeplane/propel/icons";
import type { TServiceKind } from "@plane/types";

export const SERVICE_KIND_ICON: Record<TServiceKind, ComponentType<{ className?: string }>> = {
  service: CubeOutline,
  database: LibraryOutline,
  server: ServerOutline,
  cache: CacheOutline,
  queue: ListOutline,
  storage: ArchiveOutline,
  network: ConnectionsOutline,
  external_api: GlobeOutline,
  other: BoxesOutline,
};

export const SERVICE_KIND_CLUSTER_STYLE: Record<TServiceKind, string> = {
  service: "border-kind-service/25 bg-kind-service/[0.035]",
  database: "border-kind-database/25 bg-kind-database/[0.035]",
  server: "border-kind-server/25 bg-kind-server/[0.035]",
  cache: "border-kind-cache/25 bg-kind-cache/[0.035]",
  queue: "border-kind-queue/25 bg-kind-queue/[0.035]",
  storage: "border-kind-storage/25 bg-kind-storage/[0.035]",
  network: "border-kind-network/25 bg-kind-network/[0.035]",
  external_api: "border-kind-external-api/25 bg-kind-external-api/[0.035]",
  other: "border-kind-other/20 bg-kind-other/[0.025]",
};

export const RESTRICTED_CLUSTER_STYLE = "border-dashed border-subtle bg-layer-2/20";
