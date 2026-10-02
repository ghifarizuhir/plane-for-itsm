import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { getReleaseLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { RocketOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { CreateReleaseModal } from "../create-release-modal";
import { DeleteReleaseModal } from "../delete-release-modal";
// hooks
import { useRelease } from "@/hooks/store/use-release";

export const ReleaseDetailBreadcrumbs = observer(function ReleaseDetailBreadcrumbs() {
  const { workspaceSlug, releaseId } = useParams();
  const { t } = useTranslation();
  const { getReleaseById } = useRelease();
  const [isEditOpen, setIsEditOpen] = useState(false);
  const [isDeleteOpen, setIsDeleteOpen] = useState(false);

  const slug = workspaceSlug?.toString() ?? "";
  const release = releaseId ? getReleaseById(releaseId.toString()) : null;

  return (
    <>
      <Header>
        <Header.LeftItem>
          <Breadcrumbs>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("release.title")}
                  href={`/${slug}/releases`}
                  icon={<RocketOutline className="h-4 w-4 text-tertiary" />}
                  isLast={!release}
                />
              }
              isLast={!release}
            />
            {release && (
              <Breadcrumbs.Item
                component={<BreadcrumbLink label={release.name} href={getReleaseLink(slug, release.id)} isLast />}
                isLast
              />
            )}
          </Breadcrumbs>
        </Header.LeftItem>
        <Header.RightItem>
          {release && (
            <div className="flex items-center gap-2">
              <Button variant="secondary" size="lg" onClick={() => setIsEditOpen(true)}>
                {t("release.edit")}
              </Button>
              <Button
                variant="secondary"
                size="lg"
                className="text-danger-primary"
                onClick={() => setIsDeleteOpen(true)}
              >
                {t("release.delete")}
              </Button>
            </div>
          )}
        </Header.RightItem>
      </Header>
      {release && (
        <>
          <CreateReleaseModal
            isOpen={isEditOpen}
            onClose={() => setIsEditOpen(false)}
            workspaceSlug={slug}
            data={release}
          />
          <DeleteReleaseModal
            isOpen={isDeleteOpen}
            onClose={() => setIsDeleteOpen(false)}
            workspaceSlug={slug}
            release={release}
          />
        </>
      )}
    </>
  );
});
