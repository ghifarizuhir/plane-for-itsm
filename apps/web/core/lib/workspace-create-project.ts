/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const resolveWorkspaceCreateProject = (
  scopedProjectId: string | null,
  pickedProjectId: string | null
): string | null => scopedProjectId ?? pickedProjectId;
