/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// icons
import {
  ActivityOutline,
  ClockOutline,
  DocumentationOutline,
  FlagOutline,
  LinkOutline,
  ModuleOutline,
  StateOutline,
  UserOutline,
} from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService } from "@plane/types";
// components
import { SidebarPropertyListItem } from "@/components/common/layout/sidebar/property-list-item";
import { MemberDropdown } from "@/components/dropdowns/member/dropdown";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// local imports
import { ServiceDeployCell } from "../health/service-deploy-cell";
import { ServiceHealthPill } from "../health/service-health-pill";
import { ServiceDependencies } from "./dependencies";
import { ServicePropertySelect } from "./property-select";
import { ServiceUrlProperty } from "./url-property";

const SERVICE_STATUS_OPTIONS: IService["status"][] = ["active", "planned", "maintenance", "deprecated", "retired"];
const SERVICE_CRITICALITY_OPTIONS: IService["criticality"][] = ["critical", "high", "medium", "low"];
const SERVICE_TYPE_OPTIONS: IService["type"][] = ["internal", "external", "infrastructure", "third_party"];

type Props = {
  serviceId: string;
};

export const ServiceDetailSidebar = observer(function ServiceDetailSidebar(props: Props) {
  const { serviceId } = props;
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, getServiceHealth, updateService } = useService();
  const { getWorkspaceBySlug } = useWorkspace();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString() ?? "";
  const pid = projectId?.toString() ?? service.project_id;
  const workspaceId = getWorkspaceBySlug(slug)?.id;
  const health = getServiceHealth(serviceId);

  const update = async (data: Partial<IService>) => {
    if (!slug || !workspaceId) return;
    try {
      await updateService(slug, workspaceId, pid, serviceId, data);
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("common.error.label"),
        message: t("entity.update.failed", { entity: t("service.title") }),
      });
    }
  };

  return (
    <div className="w-full px-6 md:h-full md:overflow-y-auto">
      <h5 className="mt-5 text-body-xs-medium">{t("common.properties")}</h5>
      <div className="mt-4 mb-2 space-y-2.5 truncate">
        <SidebarPropertyListItem icon={ActivityOutline} label={t("service.fields.health")}>
          <div className="flex flex-wrap items-center gap-2 px-2">
            <ServiceHealthPill health={health?.health} />
            {(health?.incidents.length ?? 0) > 0 && (
              <span className="text-11 font-medium text-danger-primary">{t("service.incidents.active")}</span>
            )}
          </div>
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={ClockOutline} label={t("service.fields.deploy")}>
          <div className="flex items-center px-2">
            <ServiceDeployCell lastDeployedAt={health?.last_deployed_at ?? null} />
          </div>
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={StateOutline} label={t("service.fields.status")}>
          <ServicePropertySelect
            value={service.status}
            options={SERVICE_STATUS_OPTIONS}
            i18nPrefix="service.status_values"
            onChange={(val) => update({ status: val })}
          />
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={FlagOutline} label={t("service.fields.criticality")}>
          <ServicePropertySelect
            value={service.criticality}
            options={SERVICE_CRITICALITY_OPTIONS}
            i18nPrefix="service.criticality_values"
            onChange={(val) => update({ criticality: val })}
          />
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={ModuleOutline} label={t("service.fields.type")}>
          <ServicePropertySelect
            value={service.type}
            options={SERVICE_TYPE_OPTIONS}
            i18nPrefix="service.type_values"
            onChange={(val) => update({ type: val })}
          />
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={UserOutline} label={t("service.fields.owner")}>
          <MemberDropdown
            value={service.owner_id}
            onChange={(val) => update({ owner_id: val })}
            projectId={pid}
            multiple={false}
            placeholder={t("service.fields.owner")}
            buttonVariant="transparent-with-text"
            className="group w-full grow"
            buttonContainerClassName="w-full text-left h-7.5"
            buttonClassName="text-body-xs-regular"
            dropdownArrow
            dropdownArrowClassName="h-3.5 w-3.5 hidden group-hover:inline"
          />
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={LinkOutline} label={t("service.fields.repository")}>
          <ServiceUrlProperty value={service.repository_url} onSubmit={(val) => update({ repository_url: val })} />
        </SidebarPropertyListItem>

        <SidebarPropertyListItem icon={DocumentationOutline} label={t("service.fields.documentation")}>
          <ServiceUrlProperty
            value={service.documentation_url}
            onSubmit={(val) => update({ documentation_url: val })}
          />
        </SidebarPropertyListItem>
      </div>

      <div className="mt-5 border-t border-subtle pt-4 pb-5">
        <h5 className="text-body-xs-medium">{t("common.dependencies")}</h5>
        <ServiceDependencies serviceId={serviceId} />
      </div>
    </div>
  );
});
