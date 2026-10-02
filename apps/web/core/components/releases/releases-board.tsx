import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { ReleaseBoardRow } from "./release-board-row";

type Props = {
  workspaceSlug: string;
  releaseIds: string[];
};

export const ReleasesBoard = observer(function ReleasesBoard({ workspaceSlug, releaseIds }: Props) {
  const { t } = useTranslation();

  return (
    <div className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto">
      <div className="sticky top-0 z-10 flex items-center gap-3 border-b border-subtle bg-surface-1 px-4 py-2 text-11 font-medium text-tertiary">
        <span className="w-16 shrink-0">&nbsp;</span>
        <span className="flex-1">{t("release.fields.name")}</span>
        <span className="w-24 text-center">{t("release.fields.status")}</span>
        <span className="w-28 text-right">{t("release.fields.target_date")}</span>
        <span className="w-3.5" />
      </div>
      {releaseIds.map((releaseId) => (
        <ReleaseBoardRow key={releaseId} releaseId={releaseId} workspaceSlug={workspaceSlug} />
      ))}
    </div>
  );
});
