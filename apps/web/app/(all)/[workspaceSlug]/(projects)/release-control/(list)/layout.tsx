import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { ReleaseControlHeader } from "@/components/reviews";

export default function WorkspaceReleaseControlLayout() {
  return (
    <>
      <AppHeader header={<ReleaseControlHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
