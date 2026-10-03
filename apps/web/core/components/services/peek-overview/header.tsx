/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import Link from "next/link";
// icons
import {
  ArrowNarrowRightOutline,
  DragDropOutline,
  FullScreenPeekOutline,
  ModalPeekOutline,
  SidePeekOutline,
} from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { IService, TNameDescriptionLoader } from "@plane/types";
import { CustomSelect } from "@plane/ui";
import { cn } from "@plane/utils";
// components
import { NameDescriptionUpdateStatus } from "@/components/issues/issue-update-status";
// local imports
import { ServiceDetailQuickActions } from "../detail/quick-actions";

export type TServicePeekModes = "side-peek" | "modal" | "full-screen";

const PEEK_OPTIONS: { key: TServicePeekModes; icon: typeof SidePeekOutline; i18n_title: string }[] = [
  { key: "side-peek", icon: SidePeekOutline, i18n_title: "common.side_peek" },
  { key: "modal", icon: ModalPeekOutline, i18n_title: "common.modal" },
  { key: "full-screen", icon: FullScreenPeekOutline, i18n_title: "common.full_screen" },
];

type Props = {
  peekMode: TServicePeekModes;
  setPeekMode: (mode: TServicePeekModes) => void;
  closePeek: () => void;
  serviceLink: string;
  service?: IService;
  isSubmitting: TNameDescriptionLoader;
};

export function ServicePeekHeader(props: Props) {
  const { peekMode, setPeekMode, closePeek, serviceLink, service, isSubmitting } = props;
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const currentMode = PEEK_OPTIONS.find((mode) => mode.key === peekMode) ?? PEEK_OPTIONS[0];

  return (
    <div className="relative flex items-center justify-between p-4">
      <div className="flex items-center gap-4">
        <Tooltip label={t("common.close_peek_view")}>
          <button type="button" onClick={closePeek}>
            <ArrowNarrowRightOutline className="h-4 w-4 text-tertiary hover:text-secondary" />
          </button>
        </Tooltip>
        <Tooltip label={t("common.open_in_full_screen", { page: t("service.title") })}>
          <Link href={serviceLink} onClick={closePeek}>
            <DragDropOutline className="h-4 w-4 text-tertiary hover:text-secondary" />
          </Link>
        </Tooltip>
        <CustomSelect
          value={peekMode}
          onChange={(value: TServicePeekModes) => setPeekMode(value)}
          customButton={
            <Tooltip label={t("common.toggle_peek_view_layout")}>
              <button type="button">
                <currentMode.icon className="h-4 w-4 text-tertiary hover:text-secondary" />
              </button>
            </Tooltip>
          }
        >
          {PEEK_OPTIONS.map((mode) => (
            <CustomSelect.Option key={mode.key} value={mode.key}>
              <div
                className={cn(
                  "flex items-center gap-1.5",
                  mode.key === currentMode.key ? "text-secondary" : "text-placeholder hover:text-secondary"
                )}
              >
                <mode.icon className="-my-1 h-4 w-4 flex-shrink-0" />
                {t(mode.i18n_title)}
              </div>
            </CustomSelect.Option>
          ))}
        </CustomSelect>
      </div>
      {service && (
        <div className="flex items-center gap-x-4">
          <NameDescriptionUpdateStatus isSubmitting={isSubmitting} />
          <ServiceDetailQuickActions serviceId={service.id} />
        </div>
      )}
    </div>
  );
}
