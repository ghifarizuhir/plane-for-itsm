import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ReviewBoardRoot } from "@/components/reviews";

function WorkspaceReleaseControlPage() {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();

  if (!workspaceSlug) return null;

  return (
    <>
      <PageHead title={t("sidebar.release_control")} />
      <div className="relative h-full w-full overflow-hidden">
        <ReviewBoardRoot workspaceSlug={workspaceSlug.toString()} boardType="rcb" />
      </div>
    </>
  );
}

export default observer(WorkspaceReleaseControlPage);
