/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { EUserPermissions, EUserPermissionsLevel, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { AlertOctagonOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { CommonProjectBreadcrumbs } from "@/components/breadcrumbs/common";
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { CreateWarRoomModal } from "../create/create-war-room-modal";
import { WarRoomSearchInput } from "./war-room-search-input";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";

export const WarRoomViewHeader = observer(function WarRoomViewHeader() {
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { loader } = useProject();
  const { allowPermissions } = useUserPermissions();
  // states
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  // derived values
  const canCreateWarRoom = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.PROJECT
  );

  return (
    <Header>
      <Header.LeftItem>
        <div>
          <Breadcrumbs isLoading={loader === "init-loader"}>
            <CommonProjectBreadcrumbs workspaceSlug={workspaceSlug?.toString()} projectId={projectId?.toString()} />
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("war_room.title")}
                  href={getWarRoomLink(workspaceSlug?.toString() ?? "", projectId?.toString() ?? "")}
                  icon={<AlertOctagonOutline className="h-4 w-4 text-tertiary" />}
                  isLast
                />
              }
              isLast
            />
          </Breadcrumbs>
        </div>
      </Header.LeftItem>
      <Header.RightItem>
        <div className="flex h-full items-center gap-2 self-end">
          <WarRoomSearchInput />
        </div>
        {canCreateWarRoom && (
          <Button variant="primary" onClick={() => setIsCreateModalOpen(true)} size="lg">
            <div className="block sm:hidden">{t("add")}</div>
            <div className="hidden sm:block">{t("war_room.add")}</div>
          </Button>
        )}
      </Header.RightItem>
      {workspaceSlug && projectId && (
        <CreateWarRoomModal
          isOpen={isCreateModalOpen}
          onClose={() => setIsCreateModalOpen(false)}
          workspaceSlug={workspaceSlug.toString()}
          projectId={projectId.toString()}
        />
      )}
    </Header>
  );
});
