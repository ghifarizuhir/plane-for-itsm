/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import { TabNavigationItem, TabNavigationList } from "@plane/propel/tab-navigation";

export type TServiceDetailTab = "overview" | "work_items" | "dependencies";

const SERVICE_DETAIL_TABS: { key: TServiceDetailTab; i18n_key: string }[] = [
  { key: "overview", i18n_key: "service.tabs.overview" },
  { key: "work_items", i18n_key: "service.tabs.work_items" },
  { key: "dependencies", i18n_key: "service.tabs.dependencies" },
];

type Props = {
  activeTab: TServiceDetailTab;
  onChange: (tab: TServiceDetailTab) => void;
};

export function ServiceDetailTabs(props: Props) {
  const { activeTab, onChange } = props;
  // plane hooks
  const { t } = useTranslation();

  return (
    <TabNavigationList className="border-b border-subtle">
      {SERVICE_DETAIL_TABS.map((tab) => (
        <button key={tab.key} type="button" onClick={() => onChange(tab.key)}>
          <TabNavigationItem isActive={activeTab === tab.key}>
            <span>{t(tab.i18n_key)}</span>
          </TabNavigationItem>
        </button>
      ))}
    </TabNavigationList>
  );
}
