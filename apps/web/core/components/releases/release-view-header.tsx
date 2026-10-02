import { useState } from "react";
import { observer } from "mobx-react";
import { useParams, useSearchParams } from "next/navigation";
// plane imports
import { EUserPermissions, EUserPermissionsLevel, RELEASE_STATUS_CONFIG, RELEASE_STATUSES } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { CloseOutline, RocketOutline, SearchOutline } from "@makeplane/propel/icons";
import type { TReleaseStatus } from "@plane/types";
import { Breadcrumbs, CustomSelect, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { CreateReleaseModal } from "./create-release-modal";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useUserPermissions } from "@/hooks/store/user";

export const ReleaseViewHeader = observer(function ReleaseViewHeader() {
  const { workspaceSlug } = useParams();
  const router = useAppRouter();
  const searchParams = useSearchParams();
  const { t } = useTranslation();
  const { allowPermissions } = useUserPermissions();
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  const canCreateRelease = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.WORKSPACE
  );
  const query = searchParams.get("q") ?? "";
  const status = searchParams.get("status") ?? "";

  const updateParams = (updates: { q?: string; status?: string }) => {
    const params = new URLSearchParams(searchParams.toString());
    Object.entries(updates).forEach(([key, value]) => {
      if (value) params.set(key, value);
      else params.delete(key);
    });
    const queryString = params.toString();
    router.replace(`/${workspaceSlug}/releases${queryString ? `?${queryString}` : ""}`);
  };

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("release.title")}
                href={`/${workspaceSlug}/releases`}
                icon={<RocketOutline className="h-4 w-4 text-tertiary" />}
                isLast
              />
            }
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
      <Header.RightItem>
        <div className="flex h-full items-center gap-2 self-end">
          <div className="flex items-center gap-1 rounded-md border border-subtle px-2">
            <SearchOutline className="h-3.5 w-3.5 text-tertiary" />
            <input
              value={query}
              onChange={(event) => updateParams({ q: event.target.value })}
              placeholder={t("release.search_placeholder")}
              className="h-7 w-40 bg-transparent text-13 outline-none"
            />
            {query && (
              <button type="button" onClick={() => updateParams({ q: "" })}>
                <CloseOutline className="h-3.5 w-3.5 text-tertiary" />
              </button>
            )}
          </div>
          <CustomSelect
            value={status}
            onChange={(value: string) => updateParams({ status: value })}
            label={status ? t(RELEASE_STATUS_CONFIG[status as TReleaseStatus].label_key) : t("release.all_statuses")}
          >
            <CustomSelect.Option value="">{t("release.all_statuses")}</CustomSelect.Option>
            {RELEASE_STATUSES.map((value) => (
              <CustomSelect.Option key={value} value={value}>
                {t(RELEASE_STATUS_CONFIG[value].label_key)}
              </CustomSelect.Option>
            ))}
          </CustomSelect>
        </div>
        {canCreateRelease && (
          <Button variant="primary" onClick={() => setIsCreateModalOpen(true)} size="lg">
            {t("release.add")}
          </Button>
        )}
      </Header.RightItem>
      {workspaceSlug && (
        <CreateReleaseModal
          isOpen={isCreateModalOpen}
          onClose={() => setIsCreateModalOpen(false)}
          workspaceSlug={workspaceSlug.toString()}
        />
      )}
    </Header>
  );
});
