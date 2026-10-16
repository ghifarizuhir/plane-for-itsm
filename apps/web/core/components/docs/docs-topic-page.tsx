/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Suspense } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { ChevronLeftOutline, ChevronRightOutline } from "@makeplane/propel/icons";
import { cn } from "@plane/utils";
// components
import { PageHead } from "@/components/core/page-title";
// local imports
import { DOC_TOPIC_GROUPS, getTopicNeighbors } from "./registry";
import type { TDocTopic } from "./registry";

export function DocsTopicPage({ topic }: { topic: TDocTopic }) {
  const { workspaceSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const { prev, next } = getTopicNeighbors(topic.slug);
  const group = DOC_TOPIC_GROUPS.find((item) => item.key === topic.group);
  const Content = topic.component;

  return (
    <div className="h-full w-full overflow-y-auto">
      <PageHead title={`${topic.title} · Docs`} />
      <div className="mx-auto w-full max-w-3xl px-6 py-10 md:px-10">
        {group && <p className="tracking-wider text-11 font-semibold text-tertiary uppercase">{group.label}</p>}
        <h1 className={cn("text-20 font-semibold text-primary", group && "mt-2")}>{topic.title}</h1>
        <p className="mt-2 text-13 leading-6 text-secondary">{topic.description}</p>
        <div className="mt-8">
          <Suspense fallback={<div className="h-40 w-full animate-pulse rounded-lg bg-layer-1" />}>
            <Content />
          </Suspense>
        </div>
        {(prev || next) && (
          <div className="mt-12 flex items-stretch justify-between gap-4 border-t border-subtle pt-6">
            {prev ? (
              <Link
                href={`/${slug}/docs/${prev.slug}`}
                className="flex max-w-[45%] flex-col gap-1 rounded-lg border border-subtle p-3 hover:bg-layer-transparent-hover"
              >
                <span className="flex items-center gap-1 text-11 text-tertiary">
                  <ChevronLeftOutline className="size-3 flex-shrink-0" />
                  Sebelumnya
                </span>
                <span className="text-13 font-medium text-primary">{prev.title}</span>
              </Link>
            ) : (
              <div />
            )}
            {next && (
              <Link
                href={`/${slug}/docs/${next.slug}`}
                className="ml-auto flex max-w-[45%] flex-col items-end gap-1 rounded-lg border border-subtle p-3 hover:bg-layer-transparent-hover"
              >
                <span className="flex items-center gap-1 text-11 text-tertiary">
                  Selanjutnya
                  <ChevronRightOutline className="size-3 flex-shrink-0" />
                </span>
                <span className="text-13 font-medium text-primary">{next.title}</span>
              </Link>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
