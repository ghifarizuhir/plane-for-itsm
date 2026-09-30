/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useContext } from "react";
// mobx store
import { StoreContext } from "@/lib/store-context";
// types
import type { IWarRoomFilterStore } from "@/store/war-room_filter.store";

export const useWarRoomFilter = (): IWarRoomFilterStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useWarRoomFilter must be used within StoreProvider");
  return context.warRoomFilter;
};
