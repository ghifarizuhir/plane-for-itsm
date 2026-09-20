/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React from "react";
import { AuthRoot } from "@/components/account/auth-forms/auth-root";
import type { EAuthModes } from "@/helpers/authentication.helper";
import { AuthFooter } from "./footer";
import { AuthHeader } from "./header";
import { AuthWavesPanel } from "./shape-waves/waves-panel";

type AuthBaseProps = {
  authType: EAuthModes;
};

export function AuthBase({ authType }: AuthBaseProps) {
  return (
    <div className="relative z-10 flex h-screen w-screen overflow-hidden bg-surface-1">
      <div className="flex h-full w-full min-w-0 flex-col overflow-hidden overflow-y-auto px-6 pt-6 pb-10 sm:px-8 lg:w-[46%] lg:min-w-[30rem] xl:min-w-[34rem]">
        <AuthHeader type={authType} />
        <AuthRoot authMode={authType} />
        <AuthFooter />
      </div>
      <AuthWavesPanel />
    </div>
  );
}
