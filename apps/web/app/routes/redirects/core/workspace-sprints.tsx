/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { redirect } from "react-router";
import type { Route } from "./+types/workspace-sprints";

export const clientLoader = ({ params, request }: Route.ClientLoaderArgs) => {
  const { workspaceSlug } = params;
  const search = new URL(request.url).search;
  throw redirect(`/${workspaceSlug}/cycles${search}`);
};

export default function WorkspaceSprints() {
  return null;
}
