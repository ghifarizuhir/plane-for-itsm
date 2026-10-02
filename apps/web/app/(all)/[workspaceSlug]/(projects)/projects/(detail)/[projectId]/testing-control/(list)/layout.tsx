import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { TestingControlHeader } from "@/components/reviews";

export default function TestingControlLayout() {
  return (
    <>
      <AppHeader header={<TestingControlHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
