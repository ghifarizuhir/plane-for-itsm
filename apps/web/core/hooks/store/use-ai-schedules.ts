/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useContext } from "react";
// store
import { StoreContext } from "@/lib/store-context";
import type { IAiSchedulesStore } from "@/store/ai-schedules.store";

export const useAiSchedules = (): IAiSchedulesStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useAiSchedules must be used within StoreProvider");
  return context.aiSchedules;
};
