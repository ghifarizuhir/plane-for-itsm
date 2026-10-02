import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { ReleasesListView } from "@/components/releases";

function WorkspaceReleasesPage() {
  const { t } = useTranslation();

  return (
    <>
      <PageHead title={t("release.title")} />
      <div className="relative h-full w-full overflow-hidden">
        <ReleasesListView />
      </div>
    </>
  );
}

export default observer(WorkspaceReleasesPage);
