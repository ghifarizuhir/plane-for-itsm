/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import Link from "next/link";
import { useParams } from "next/navigation";
import { ChevronDownOutline } from "@makeplane/propel/icons";
import { Collapsible } from "@plane/ui";
import { cn } from "@plane/utils";
import { DEFAULT_DOC_TOPIC_SLUG, DOC_TOPIC_GROUPS, DOC_TOPICS } from "./registry";
import { useDocsSidebarGroups } from "./use-docs-sidebar-groups";

type Props = {
  isMobile?: boolean;
  onNavigate?: () => void;
};

export function DocsSidebar({ isMobile = false, onNavigate }: Props) {
  const { workspaceSlug, topicSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const activeSlug = topicSlug?.toString() ?? DEFAULT_DOC_TOPIC_SLUG;
  const { isGroupOpen, toggleGroup } = useDocsSidebarGroups();

  return (
    <nav
      className={cn(
        "h-full w-64 flex-col overflow-y-auto px-3 py-4",
        isMobile ? "flex w-full" : "hidden border-r border-subtle md:flex"
      )}
    >
      {DOC_TOPIC_GROUPS.map((group) => {
        const isOpen = isGroupOpen(group.key);
        return (
          <Collapsible
            key={group.key}
            className="mb-4 last:mb-0"
            buttonClassName="flex w-full items-center justify-between gap-2 rounded-md px-2 py-1.5 hover:bg-layer-transparent-hover"
            isOpen={isOpen}
            onToggle={() => toggleGroup(group.key)}
            title={
              <>
                <span className="tracking-wider text-11 font-semibold text-placeholder uppercase">{group.label}</span>
                <ChevronDownOutline
                  className={cn(
                    "size-3.5 flex-shrink-0 text-placeholder transition-transform",
                    !isOpen && "-rotate-90"
                  )}
                />
              </>
            }
          >
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
          </Collapsible>
        );
      })}
    </nav>
  );
}
