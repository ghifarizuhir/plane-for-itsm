/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useState } from "react";

/**
 * Click-only focus. The inspector opens when a service is clicked and stays
 * open until it is closed or the canvas is clicked.
 */
export function useServiceFocusInteraction() {
  const [pinnedServiceId, setPinnedServiceId] = useState<string | null>(null);

  const pin = useCallback((serviceId: string) => {
    setPinnedServiceId(serviceId);
  }, []);

  const clear = useCallback(() => {
    setPinnedServiceId(null);
  }, []);

  return {
    effectiveServiceId: pinnedServiceId,
    pinnedServiceId,
    pin,
    clear,
  };
}
