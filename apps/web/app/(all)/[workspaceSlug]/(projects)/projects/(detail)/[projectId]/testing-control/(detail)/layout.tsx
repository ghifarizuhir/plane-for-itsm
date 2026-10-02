import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { SessionDetailBreadcrumbs } from "@/components/reviews";

export default function TestingControlDetailLayout() {
  return (
    <>
      <AppHeader header={<SessionDetailBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
