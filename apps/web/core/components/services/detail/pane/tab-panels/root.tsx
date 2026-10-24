/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { Tabs } from "@plane/propel/tabs";
// local imports
import { SERVICE_DETAIL_PANE_TABS } from "../helpers";
import { ServicePropertiesTabPanel } from "./properties";
import { ServiceRelationshipsTabPanel } from "./relationships";
import { ServiceTicketsTabPanel } from "./tickets";
import { ServiceMonitoringTabPanel } from "./monitoring";

type Props = {
  serviceId: string;
};

export function ServiceDetailPaneTabPanelsRoot(props: Props) {
  const { serviceId } = props;

  return (
    <>
      {SERVICE_DETAIL_PANE_TABS.map((tab) => (
        <Tabs.Content key={tab.key} value={tab.key} className="flex-1 overflow-y-auto p-3.5">
          {tab.key === "properties" && <ServicePropertiesTabPanel serviceId={serviceId} />}
          {tab.key === "relationships" && <ServiceRelationshipsTabPanel serviceId={serviceId} />}
          {tab.key === "tickets" && <ServiceTicketsTabPanel serviceId={serviceId} />}
          {tab.key === "monitoring" && <ServiceMonitoringTabPanel serviceId={serviceId} />}
        </Tabs.Content>
      ))}
    </>
  );
}
