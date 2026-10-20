/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// local imports
import { ServiceRelationships } from "../../relationships";

type Props = {
  serviceId: string;
};

export function ServiceRelationshipsTabPanel(props: Props) {
  const { serviceId } = props;

  return <ServiceRelationships serviceId={serviceId} layout="pane" />;
}
