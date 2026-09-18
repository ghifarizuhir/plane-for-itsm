/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
// ui
import { getButtonStyling } from "@plane/propel/button";
import { PlaneLogo } from "@plane/propel/icons";
// helpers
import { cn } from "@plane/utils";

export function ProductUpdatesFooter() {
  const { t } = useTranslation();
  return (
    <div className="m-6 mb-4 flex flex-shrink-0 items-center justify-between gap-4">
      <div className="flex items-center gap-2">
        <a
          href="mailto:support@terraline.space"
          target="_blank"
          className="text-13 text-secondary underline-offset-1 outline-none hover:text-primary hover:underline"
          rel="noreferrer"
        >
          {t("support")}
        </a>
      </div>
      <a
        href="https://terraline.space/pages"
        target="_blank"
        className={cn(
          getButtonStyling("secondary", "base"),
          "flex items-center gap-1.5 text-center font-medium underline-offset-2 outline-none hover:underline"
        )}
        rel="noreferrer"
      >
        <PlaneLogo className="h-4 w-auto text-primary" />
        {t("powered_by_plane_pages")}
      </a>
    </div>
  );
}
