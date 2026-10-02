import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { ReleaseDetailBreadcrumbs } from "@/components/releases";

export default function WorkspaceReleaseDetailLayout() {
  return (
    <>
      <AppHeader header={<ReleaseDetailBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
