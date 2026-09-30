/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { action, observable, makeObservable, reaction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type { TWarRoomStatusTab } from "@plane/types";
// store
import type { CoreRootStore } from "./root.store";

export interface IWarRoomFilterStore {
  statusTabs: Record<string, TWarRoomStatusTab>;
  searchQuery: string;
  getStatusTab: (projectId: string) => TWarRoomStatusTab;
  setStatusTab: (projectId: string, tab: TWarRoomStatusTab) => void;
  updateSearchQuery: (query: string) => void;
}

export class WarRoomFilterStore implements IWarRoomFilterStore {
  statusTabs: Record<string, TWarRoomStatusTab> = {};
  searchQuery: string = "";
  rootStore: CoreRootStore;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      statusTabs: observable,
      searchQuery: observable.ref,
      setStatusTab: action,
      updateSearchQuery: action,
    });
    this.rootStore = _rootStore;

    reaction(
      () => this.rootStore.router.projectId,
      () => {
        this.searchQuery = "";
      }
    );
  }

  getStatusTab = computedFn((projectId: string): TWarRoomStatusTab => this.statusTabs[projectId] ?? "active");

  setStatusTab = (projectId: string, tab: TWarRoomStatusTab) => {
    this.statusTabs[projectId] = tab;
  };

  updateSearchQuery = (query: string) => {
    this.searchQuery = query;
  };
}
