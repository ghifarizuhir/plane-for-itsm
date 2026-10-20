/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React from "react";
import { AuthRoot } from "@/components/account/auth-forms/auth-root";
import { EAuthModes } from "@/helpers/authentication.helper";
import { AuthHeader } from "./header";
import { AuthSignupCopy, AuthSignupTrust } from "./signup-copy";

type AuthBaseProps = {
  authType: EAuthModes;
};

export function AuthBase({ authType }: AuthBaseProps) {
  return (
    <div className="relative z-10 flex h-screen w-screen justify-center overflow-hidden bg-surface-1">
      <div className="mx-auto flex h-full w-full max-w-[28rem] min-w-0 flex-col overflow-hidden overflow-y-auto px-6 pt-6 pb-10 sm:px-8 lg:max-w-[36rem]">
        <AuthHeader type={authType} />
        {authType === EAuthModes.SIGN_UP && (
          <div className="lg:hidden">
            <AuthSignupCopy kind="signup" />
          </div>
        )}
        {authType === EAuthModes.SIGN_IN && (
          <div className="lg:hidden">
            <AuthSignupCopy kind="signin" />
          </div>
        )}
        <AuthRoot authMode={authType} />
        {(authType === EAuthModes.SIGN_UP || authType === EAuthModes.SIGN_IN) && (
          <div className="lg:hidden">
            <AuthSignupTrust />
          </div>
        )}
      </div>
    </div>
  );
}
