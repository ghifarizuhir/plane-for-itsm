/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TServiceDetailPaneTab = "properties" | "relationships" | "tickets";

export const SERVICE_DETAIL_PANE_WIDTH = 320;

export const SERVICE_DETAIL_PANE_TABS_QUERY_PARAM = "paneTab";

export const SERVICE_DETAIL_PANE_TABS: {
  key: TServiceDetailPaneTab;
  i18n_label: string;
}[] = [
  { key: "properties", i18n_label: "common.properties" },
  { key: "relationships", i18n_label: "service.relationships.title" },
  { key: "tickets", i18n_label: "service.tabs.work_items" },
];

export const SERVICE_DETAIL_PANE_TAB_KEYS = SERVICE_DETAIL_PANE_TABS.map((tab) => tab.key);

export const DEFAULT_SERVICE_DETAIL_PANE_TAB: TServiceDetailPaneTab = "properties";

export const isServiceDetailPaneTab = (value: string | null | undefined): value is TServiceDetailPaneTab =>
  Boolean(value) && SERVICE_DETAIL_PANE_TAB_KEYS.includes(value as TServiceDetailPaneTab);

export const resolveServiceDetailPaneTab = (value: string | null | undefined): TServiceDetailPaneTab | null =>
  isServiceDetailPaneTab(value) ? value : null;
