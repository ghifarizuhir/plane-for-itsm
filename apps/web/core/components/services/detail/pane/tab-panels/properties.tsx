/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// local imports
import { ServiceProjectLinks } from "../../project-links";
import { ServiceDetailSidebar } from "../../sidebar";

type Props = {
  serviceId: string;
};

export function ServicePropertiesTabPanel(props: Props) {
  const { serviceId } = props;

  return (
    <div className="flex flex-col gap-4">
      <ServiceDetailSidebar serviceId={serviceId} layout="pane" />
      <div className="border-t border-subtle pt-4">
        <ServiceProjectLinks serviceId={serviceId} />
      </div>
    </div>
  );
}
