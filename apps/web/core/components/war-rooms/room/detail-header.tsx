/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { AlertOctagonOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { CommonProjectBreadcrumbs } from "@/components/breadcrumbs/common";
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useAppRouter } from "@/hooks/use-app-router";

export const WarRoomDetailHeader = observer(function WarRoomDetailHeader() {
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId, warRoomId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getWarRoomDetailById } = useWarRoom();
  // derived values
  const room = warRoomId ? getWarRoomDetailById(warRoomId.toString()) : null;

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs onBack={router.back}>
          <CommonProjectBreadcrumbs workspaceSlug={workspaceSlug?.toString()} projectId={projectId?.toString()} />
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("war_room.title")}
                href={getWarRoomLink(workspaceSlug?.toString() ?? "", projectId?.toString() ?? "")}
                icon={<AlertOctagonOutline className="h-4 w-4 text-tertiary" />}
              />
            }
          />
          <Breadcrumbs.Item
            component={<BreadcrumbLink label={room ? `WR-${room.sequence_id} · ${room.name}` : ""} isLast />}
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});
