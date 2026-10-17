/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useMemo, useState } from "react";
import { useParams } from "next/navigation";
import {
  DOCS_SIDEBAR_GROUPS_KEY,
  parseGroupOverrides,
  resolveGroupOpen,
  serializeGroupOverrides,
  toggleGroupOverrides,
  type TGroupOverrides,
} from "./docs-sidebar-state";
import { DEFAULT_DOC_TOPIC_SLUG, getTopicBySlug, type TDocGroupKey } from "./registry";

const persistOverrides = (overrides: TGroupOverrides) => {
  try {
    window.localStorage.setItem(DOCS_SIDEBAR_GROUPS_KEY, serializeGroupOverrides(overrides));
  } catch {
    return;
  }
};

const readOverrides = (): TGroupOverrides => {
  if (typeof window === "undefined") return {};
  try {
    return parseGroupOverrides(window.localStorage.getItem(DOCS_SIDEBAR_GROUPS_KEY));
  } catch {
    return {};
  }
};

export function useDocsSidebarGroups() {
  const { topicSlug } = useParams();
  const activeSlug = topicSlug?.toString() ?? DEFAULT_DOC_TOPIC_SLUG;
  const activeGroup = useMemo<TDocGroupKey>(() => getTopicBySlug(activeSlug)?.group ?? "getting-started", [activeSlug]);

  const [overrides, setOverrides] = useState<TGroupOverrides>(readOverrides);

  const isGroupOpen = useCallback(
    (group: TDocGroupKey) => resolveGroupOpen(group, activeGroup, overrides),
    [activeGroup, overrides]
  );

  const toggleGroup = useCallback(
    (group: TDocGroupKey) => {
      setOverrides((previous) => {
        const next = toggleGroupOverrides(previous, group, !resolveGroupOpen(group, activeGroup, previous));
        persistOverrides(next);
        return next;
      });
    },
    [activeGroup]
  );

  return { isGroupOpen, toggleGroup };
}
