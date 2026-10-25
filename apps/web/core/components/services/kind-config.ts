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
  service: "border-kind-service/35 bg-kind-service/6 text-kind-service",
  database: "border-kind-database/35 bg-kind-database/6 text-kind-database",
  server: "border-kind-server/35 bg-kind-server/6 text-kind-server",
  cache: "border-kind-cache/35 bg-kind-cache/6 text-kind-cache",
  queue: "border-kind-queue/35 bg-kind-queue/6 text-kind-queue",
  storage: "border-kind-storage/35 bg-kind-storage/6 text-kind-storage",
  network: "border-kind-network/35 bg-kind-network/6 text-kind-network",
  external_api: "border-kind-external-api/35 bg-kind-external-api/6 text-kind-external-api",
  other: "border-kind-other/35 bg-kind-other/6 text-kind-other",
};

export const RESTRICTED_CLUSTER_STYLE = "border-dashed border-subtle bg-layer-2/30 text-tertiary";
