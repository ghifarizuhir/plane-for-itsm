/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useRef } from "react";
import { observer } from "mobx-react";
import { Logo } from "@plane/propel/emoji-icon-picker";
import { PagesOutline } from "@makeplane/propel/icons";
// plane imports
import { getPageName } from "@plane/utils";
// components
import { ListItem } from "@/components/core/list";
import { BlockItemAction } from "@/components/pages/list/block-item-action";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { usePlatformOS } from "@/hooks/use-platform-os";
// plane web hooks
import type { EPageStoreType } from "@/hooks/store";
import { usePage } from "@/hooks/store";

type TPageListBlock = {
  pageId: string;
  storeType: EPageStoreType;
  showProjectBadge?: boolean;
};

export const PageListBlock = observer(function PageListBlock(props: TPageListBlock) {
  const { pageId, storeType, showProjectBadge = false } = props;
  // refs
  const parentRef = useRef(null);
  // hooks
  const page = usePage({
    pageId,
    storeType,
  });
  const { getProjectById } = useProject();
  const { isMobile } = usePlatformOS();
  const project = showProjectBadge && page?.project_ids?.[0] ? getProjectById(page.project_ids[0]) : undefined;
  // handle page check
  if (!page) return null;
  // derived values
  const { name, logo_props, getRedirectionLink } = page;

  return (
    <ListItem
      prependTitleElement={
        <>
          {logo_props?.in_use ? (
            <Logo logo={logo_props} size={16} type="lucide" />
          ) : (
            <PagesOutline className="h-4 w-4 text-tertiary" />
          )}
        </>
      }
      title={getPageName(name)}
      itemLink={getRedirectionLink()}
      appendTitleElement={
        project ? (
          <span className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5 text-11 text-tertiary">
            {project.name}
          </span>
        ) : undefined
      }
      actionableItems={<BlockItemAction page={page} parentRef={parentRef} storeType={storeType} />}
      isMobile={isMobile}
      parentRef={parentRef}
    />
  );
});
