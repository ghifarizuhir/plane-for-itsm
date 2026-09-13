/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { IService } from "@plane/types";
// ui
import { TextArea } from "@plane/ui";
// hooks
import { useService } from "@/hooks/store/use-service";

type Props = {
  serviceId: string;
};

export const ServiceOverview = observer(function ServiceOverview(props: Props) {
  const { serviceId } = props;
  // router
  const { projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, getDependenciesByProject } = useService();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const pid = projectId?.toString() ?? service.project_id;
  const dependencies = getDependenciesByProject(pid);
  const dependsOn = dependencies
    .filter((d) => d.from_service_id === serviceId)
    .map((d) => getServiceById(d.to_service_id))
    .filter((s): s is IService => Boolean(s));
  const dependedOnBy = dependencies
    .filter((d) => d.to_service_id === serviceId)
    .map((d) => getServiceById(d.from_service_id))
    .filter((s): s is IService => Boolean(s));

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      {service.description && (
        <div className="flex flex-col gap-1.5">
          <h4 className="text-13 font-medium text-secondary">{t("service.fields.description")}</h4>
          <TextArea
            className="ring-none !m-0 max-h-max w-full resize-none !border-0 bg-transparent !p-0 text-13 leading-5 text-secondary outline-none"
            value={service.description}
            disabled
          />
        </div>
      )}
      <div className="flex flex-col gap-1.5">
        <h4 className="text-13 font-medium text-secondary">{t("common.dependencies")}</h4>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div className="flex flex-col gap-1.5 rounded-md border border-subtle p-3">
            <p className="text-12 font-medium text-tertiary">{t("service.detail.depends_on")}</p>
            {dependsOn.length === 0 ? (
              <p className="text-12 text-tertiary">—</p>
            ) : (
              dependsOn.map((s) => (
                <span key={s.id} className="truncate text-13 text-primary" title={s.name}>
                  {s.name}
                </span>
              ))
            )}
          </div>
          <div className="flex flex-col gap-1.5 rounded-md border border-subtle p-3">
            <p className="text-12 font-medium text-tertiary">{t("service.detail.depended_on_by")}</p>
            {dependedOnBy.length === 0 ? (
              <p className="text-12 text-tertiary">—</p>
            ) : (
              dependedOnBy.map((s) => (
                <span key={s.id} className="truncate text-13 text-primary" title={s.name}>
                  {s.name}
                </span>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
});
