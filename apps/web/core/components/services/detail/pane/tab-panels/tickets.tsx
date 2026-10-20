/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// local imports
import { ServiceWorkItems } from "../../work-items";

type Props = {
  serviceId: string;
};

export function ServiceTicketsTabPanel(props: Props) {
  const { serviceId } = props;

  return <ServiceWorkItems serviceId={serviceId} layout="pane" />;
}
