import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { sanitizeHTML } from "@plane/utils";
// components
import { ReleaseDetailChanges } from "./changes";
import { ReleaseDetailProperties } from "./properties";
import { ReleaseReviewHistory } from "./review-history";
// hooks
import { useRelease } from "@/hooks/store/use-release";
import { useUserPermissions } from "@/hooks/store/user";

type Props = {
  workspaceSlug: string;
  releaseId: string;
};

export const ReleaseDetailRoot = observer(function ReleaseDetailRoot({ workspaceSlug, releaseId }: Props) {
  const { t } = useTranslation();
  const { getReleaseDetailById, fetchReleaseDetail } = useRelease();
  const { allowPermissions } = useUserPermissions();

  const canWrite = allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.WORKSPACE);
  const release = getReleaseDetailById(releaseId);

  useEffect(() => {
    if (release) return;
    void fetchReleaseDetail(workspaceSlug, releaseId);
  }, [release, fetchReleaseDetail, releaseId, workspaceSlug]);

  if (!release) {
    return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
  }

  return (
    <div className="flex h-full w-full flex-col overflow-y-auto md:flex-row md:overflow-hidden">
      <div className="w-full space-y-6 px-9 py-5 md:h-full md:min-w-0 md:flex-1 md:overflow-y-auto">
        <div>
          <h1 className="text-16 font-semibold text-primary">{release.name}</h1>
          <div
            className="mt-2 text-13 text-secondary"
            dangerouslySetInnerHTML={{ __html: sanitizeHTML(release.description_html || "<p></p>") }}
          />
        </div>
        <ReleaseDetailChanges
          workspaceSlug={workspaceSlug}
          releaseId={releaseId}
          canWrite={canWrite}
          isUnderReview={release.status === "in_review"}
        />
        <ReleaseReviewHistory workspaceSlug={workspaceSlug} releaseId={releaseId} canWrite={canWrite} />
      </div>
      <div className="w-full shrink-0 border-t border-subtle bg-surface-1 md:h-full md:w-1/4 md:min-w-80 md:border-t-0 md:border-l xl:min-w-96">
        <ReleaseDetailProperties workspaceSlug={workspaceSlug} release={release} />
      </div>
    </div>
  );
});
