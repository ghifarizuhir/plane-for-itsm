/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { useTranslation } from "@plane/i18n";
import { Tabs } from "@plane/propel/tabs";
// local imports
import { SERVICE_DETAIL_PANE_TABS } from "./helpers";

export function ServiceDetailPaneTabsList() {
  // translation
  const { t } = useTranslation();

  return (
    <div className="mx-3.5">
      <Tabs.List>
        {SERVICE_DETAIL_PANE_TABS.map((tab) => (
          <Tabs.Trigger key={tab.key} value={tab.key} size="sm" className="min-w-0 px-1">
            {t(tab.i18n_label)}
          </Tabs.Trigger>
        ))}
        <Tabs.Indicator />
      </Tabs.List>
    </div>
  );
}
