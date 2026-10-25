/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
// local imports
import { createFocusIntentController, type TServiceFocusState } from "./focus-intent";

const INITIAL_STATE: TServiceFocusState = {
  hoveredServiceId: null,
  pinnedServiceId: null,
  suppressed: false,
};

export function useServiceFocusInteraction() {
  const [state, setState] = useState<TServiceFocusState>(INITIAL_STATE);
  const [controller] = useState(() => createFocusIntentController({ onChange: setState }));

  useEffect(
    () => () => {
      controller.dispose();
    },
    [controller]
  );

  return {
    ...state,
    effectiveServiceId: state.pinnedServiceId ?? state.hoveredServiceId,
    hover: controller.hover,
    leaveNode: controller.leaveNode,
    enterCard: controller.enterCard,
    leaveCard: controller.leaveCard,
    pin: controller.pin,
    clear: controller.clear,
    suppress: controller.suppress,
    resume: controller.resume,
  };
}
