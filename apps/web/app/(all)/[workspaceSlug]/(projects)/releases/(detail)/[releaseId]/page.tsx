import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ReleaseDetailRoot } from "@/components/releases";

export default function WorkspaceReleaseDetailPage() {
  const { workspaceSlug, releaseId } = useParams();
  const { t } = useTranslation();

  if (!workspaceSlug || !releaseId) return null;

  return (
    <>
      <PageHead title={t("release.title")} />
      <ReleaseDetailRoot workspaceSlug={workspaceSlug.toString()} releaseId={releaseId.toString()} />
    </>
  );
}
