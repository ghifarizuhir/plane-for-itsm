/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ServiceMonitoringPanel } from "@/components/services/metrics/monitoring-panel";

type Props = {
  serviceId: string;
};

export function ServiceMonitoringTabPanel({ serviceId }: Props) {
  return <ServiceMonitoringPanel serviceId={serviceId} />;
}
