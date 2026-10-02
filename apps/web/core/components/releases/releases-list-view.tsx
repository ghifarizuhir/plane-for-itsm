import { useEffect } from "react";
import { observer } from "mobx-react";
import { useParams, useSearchParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { IRelease, TReleaseStatus } from "@plane/types";
// components
import { ReleaseLoadErrorState } from "./release-load-error-state";
import { ReleasesBoard } from "./releases-board";
// helpers
import { filterReleases, orderReleases } from "@/services/release.helpers";
// hooks
import { useRelease } from "@/hooks/store/use-release";

export const ReleasesListView = observer(function ReleasesListView() {
  const { workspaceSlug } = useParams();
  const searchParams = useSearchParams();
  const { t } = useTranslation();
  const { loader, errorMap, fetchedMap, releaseMap, getWorkspaceReleaseIds, fetchReleases } = useRelease();

  const slug = workspaceSlug?.toString() ?? "";
  const query = searchParams.get("q") ?? "";
  const status = searchParams.get("status") ?? "";
  const releaseIds = getWorkspaceReleaseIds(slug);
  const hasError = errorMap[slug];

  useEffect(() => {
    if (!slug || fetchedMap[slug]) return;
    void fetchReleases(slug);
  }, [slug, fetchedMap, fetchReleases]);

  if (hasError) {
    return (
      <ReleaseLoadErrorState
        onRetry={() => {
          void fetchReleases(slug);
        }}
      />
    );
  }
  if (loader || releaseIds === null) {
    return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
  }

  const releases = releaseIds
    .map((releaseId) => releaseMap[releaseId])
    .filter((release): release is IRelease => Boolean(release));
  const filtered = orderReleases(filterReleases(releases, status ? { status: [status as TReleaseStatus] } : {}, query));

  if (releaseIds.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
        <p className="text-sm font-medium text-primary">{t("release.empty_state.title")}</p>
        <p className="text-xs text-secondary">{t("release.empty_state.description")}</p>
      </div>
    );
  }
  if (filtered.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
        <p className="text-sm font-medium text-primary">{t("release.empty_state.no_matches.title")}</p>
        <p className="text-xs text-secondary">{t("release.empty_state.no_matches.description")}</p>
      </div>
    );
  }
  return <ReleasesBoard workspaceSlug={slug} releaseIds={filtered.map((release) => release.id)} />;
});
