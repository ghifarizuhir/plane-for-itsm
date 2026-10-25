/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TServiceKind } from "@plane/types";

export const SERVICE_KIND_ORDER: TServiceKind[] = [
  "service",
  "database",
  "server",
  "cache",
  "queue",
  "storage",
  "network",
  "external_api",
  "other",
];

export const serviceKindLabelKey = (kind: TServiceKind) => `service.kind_values.${kind}`;
