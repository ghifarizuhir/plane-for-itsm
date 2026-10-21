/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { redirect } from "react-router";
import type { Route } from "./+types/workspace-sprints";

export const clientLoader = ({ params }: Route.ClientLoaderArgs) => {
  const { workspaceSlug } = params;
  throw redirect(`/${workspaceSlug}/cycles/`);
};

export default function WorkspaceSprints() {
  return null;
}
