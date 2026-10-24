/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ClockOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { calculateTimeAgo } from "@plane/utils";

type Props = {
  lastDeployedAt: string | null;
};

export function ServiceDeployCell({ lastDeployedAt }: Props) {
  const { t } = useTranslation();
  if (!lastDeployedAt) {
    return <span className="text-12 text-tertiary">{t("service.deploy.no_deployment")}</span>;
  }
  return (
    <span className="flex items-center gap-1 text-12 text-secondary">
      <ClockOutline className="h-3.5 w-3.5 text-tertiary" />
      <span className="font-code tabular-nums">{calculateTimeAgo(lastDeployedAt)}</span>
    </span>
  );
}
