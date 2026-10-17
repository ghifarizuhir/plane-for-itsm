/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TPracticeUsage } from "../blocks";

export const ITIL4_MATRIX_COLUMNS = ["Intake", "Queue", "Service", "Sprint", "Track", "KB", "War Room", "Reports"];

export const ITIL4_MATRIX_ROWS: { practice: string; cells: TPracticeUsage[] }[] = [
  {
    practice: "Incident",
    cells: ["primary", "primary", "primary", "secondary", "none", "primary", "primary", "secondary"],
  },
  {
    practice: "Problem",
    cells: ["secondary", "primary", "primary", "secondary", "primary", "primary", "none", "secondary"],
  },
  {
    practice: "Service request",
    cells: ["primary", "primary", "primary", "primary", "none", "secondary", "none", "secondary"],
  },
  {
    practice: "Change",
    cells: ["secondary", "primary", "primary", "primary", "primary", "secondary", "none", "secondary"],
  },
  {
    practice: "Improvement",
    cells: ["secondary", "primary", "secondary", "primary", "primary", "secondary", "none", "primary"],
  },
];
