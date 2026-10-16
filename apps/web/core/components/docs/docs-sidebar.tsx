/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import Link from "next/link";
import { useParams } from "next/navigation";
import { cn } from "@plane/utils";
import { DEFAULT_DOC_TOPIC_SLUG, DOC_TOPIC_GROUPS, DOC_TOPICS } from "./registry";

type Props = {
  isMobile?: boolean;
  onNavigate?: () => void;
};

export function DocsSidebar({ isMobile = false, onNavigate }: Props) {
  const { workspaceSlug, topicSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const activeSlug = topicSlug?.toString() ?? DEFAULT_DOC_TOPIC_SLUG;

  return (
    <nav
      className={cn(
        "h-full w-64 flex-col overflow-y-auto px-3 py-4",
        isMobile ? "flex w-full" : "hidden border-r border-subtle md:flex"
      )}
    >
      {DOC_TOPIC_GROUPS.map((group) => (
        <div key={group.key} className="mb-4 last:mb-0">
          <p className="tracking-wider px-2 text-11 font-semibold text-placeholder uppercase">{group.label}</p>
          <div className="mt-1 flex flex-col gap-0.5">
            {DOC_TOPICS.filter((topic) => topic.group === group.key).map((topic) => {
              const Icon = topic.icon;
              const isActive = topic.slug === activeSlug;
              return (
                <Link
                  key={topic.slug}
                  href={`/${slug}/docs/${topic.slug}`}
                  onClick={onNavigate}
                  className={cn(
                    "flex items-center gap-2 rounded-md px-2 py-1.5",
                    isActive ? "bg-layer-1 text-primary" : "text-secondary hover:bg-layer-transparent-hover"
                  )}
                >
                  <Icon className="size-4 flex-shrink-0" />
                  <span className="text-13 font-medium whitespace-nowrap">{topic.title}</span>
                </Link>
              );
            })}
          </div>
        </div>
      ))}
    </nav>
  );
}
