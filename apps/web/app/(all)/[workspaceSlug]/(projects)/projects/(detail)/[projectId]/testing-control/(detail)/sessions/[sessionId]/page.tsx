import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ReviewSessionDetailRoot } from "@/components/reviews";

function TestingControlSessionPage() {
  const { workspaceSlug, sessionId } = useParams();
  const { t } = useTranslation();

  if (!workspaceSlug || !sessionId) return null;

  return (
    <>
      <PageHead title={t("sidebar.testing_control")} />
      <ReviewSessionDetailRoot workspaceSlug={workspaceSlug.toString()} sessionId={sessionId.toString()} />
    </>
  );
}

export default observer(TestingControlSessionPage);
