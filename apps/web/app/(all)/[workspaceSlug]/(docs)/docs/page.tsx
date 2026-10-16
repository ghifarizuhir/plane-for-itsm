/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { redirect } from "react-router";
import type { Route } from "./+types/page";
// local imports
import { DEFAULT_DOC_TOPIC_SLUG } from "@/components/docs/registry";

export const clientLoader = ({ params }: Route.ClientLoaderArgs) => {
  throw redirect(`/${params.workspaceSlug}/docs/${DEFAULT_DOC_TOPIC_SLUG}/`);
};

export default function DocsIndexPage() {
  return null;
}
