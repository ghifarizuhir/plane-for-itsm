/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// hooks
import { useService } from "@/hooks/store/use-service";
// local imports
import { ServicesBoardRow } from "./services-board-row";

export const ServicesBoard = observer(function ServicesBoard() {
  // router
  const { projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getFilteredServiceIds } = useService();
  // derived values
  const serviceIds = projectId ? (getFilteredServiceIds(projectId.toString()) ?? []) : [];

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center gap-3 border-b border-subtle px-3 py-1.5 text-10 font-medium tracking-wide text-tertiary uppercase">
        <span className="flex-1">{t("service.board.service")}</span>
        <span className="hidden w-[120px] shrink-0 sm:block">{t("service.board.health")}</span>
        <span className="hidden w-[120px] shrink-0 md:block">{t("service.board.incidents")}</span>
        <span className="hidden w-[110px] shrink-0 lg:block">{t("service.board.deploy")}</span>
        <span className="hidden w-[56px] shrink-0 text-right xl:block">{t("service.board.owner")}</span>
      </div>
      <div className="vertical-scrollbar min-h-0 flex-1">
        {serviceIds.map((id) => (
          <ServicesBoardRow key={id} serviceId={id} />
        ))}
      </div>
    </div>
  );
});
