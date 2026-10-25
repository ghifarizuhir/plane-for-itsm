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
  service: "border-sky-500/40 bg-sky-500/5 text-sky-600 dark:text-sky-400",
  database: "border-violet-500/40 bg-violet-500/5 text-violet-600 dark:text-violet-400",
  server: "border-orange-500/40 bg-orange-500/5 text-orange-600 dark:text-orange-400",
  cache: "border-pink-500/40 bg-pink-500/5 text-pink-600 dark:text-pink-400",
  queue: "border-amber-500/40 bg-amber-500/5 text-amber-600 dark:text-amber-400",
  storage: "border-emerald-500/40 bg-emerald-500/5 text-emerald-600 dark:text-emerald-400",
  network: "border-cyan-500/40 bg-cyan-500/5 text-cyan-600 dark:text-cyan-400",
  external_api: "border-indigo-500/40 bg-indigo-500/5 text-indigo-600 dark:text-indigo-400",
  other: "border-subtle bg-layer-2/40 text-tertiary",
};

export const RESTRICTED_CLUSTER_STYLE = "border-dashed border-subtle bg-layer-2/30 text-tertiary";
