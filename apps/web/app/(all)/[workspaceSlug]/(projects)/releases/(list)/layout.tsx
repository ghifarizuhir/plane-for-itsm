import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { ReleaseViewHeader } from "@/components/releases";

export default function WorkspaceReleasesListLayout() {
  return (
    <>
      <AppHeader header={<ReleaseViewHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
