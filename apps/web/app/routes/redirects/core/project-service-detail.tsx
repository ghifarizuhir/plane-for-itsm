/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { redirect } from "react-router";
import type { Route } from "./+types/project-service-detail";

export const clientLoader = ({ params }: Route.ClientLoaderArgs) => {
  const { workspaceSlug, serviceId } = params;
  throw redirect(`/${workspaceSlug}/services/${serviceId}`);
};

export default function ProjectServiceDetailRedirect() {
  return null;
}
