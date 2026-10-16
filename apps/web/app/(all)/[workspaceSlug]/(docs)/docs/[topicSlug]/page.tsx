/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useParams } from "next/navigation";
import { redirect } from "react-router";
import type { Route } from "./+types/page";
// components
import { DocsTopicPage } from "@/components/docs/docs-topic-page";
// local imports
import { DEFAULT_DOC_TOPIC_SLUG, getTopicBySlug } from "@/components/docs/registry";

export const clientLoader = ({ params }: Route.ClientLoaderArgs) => {
  const topic = getTopicBySlug(params.topicSlug ?? "");
  if (!topic) throw redirect(`/${params.workspaceSlug}/docs/${DEFAULT_DOC_TOPIC_SLUG}/`);
};

export default function DocsTopicRoute() {
  const { topicSlug } = useParams();
  const topic = getTopicBySlug(topicSlug?.toString() ?? "");
  if (!topic) return null;
  return <DocsTopicPage topic={topic} />;
}
