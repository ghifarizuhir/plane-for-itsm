import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ReviewBoardRoot } from "@/components/reviews";

function TestingControlPage() {
  const { workspaceSlug, projectId } = useParams();
  const { t } = useTranslation();

  if (!workspaceSlug || !projectId) return null;

  return (
    <>
      <PageHead title={t("sidebar.testing_control")} />
      <div className="relative h-full w-full overflow-hidden">
        <ReviewBoardRoot workspaceSlug={workspaceSlug.toString()} boardType="tcb" projectId={projectId.toString()} />
      </div>
    </>
  );
}

export default observer(TestingControlPage);
