/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { HideOutline, ShowOutline } from "@makeplane/propel/icons";
// plane internal packages
import type { EAdminAuthErrorCodes, TAdminAuthErrorInfo } from "@plane/constants";
import { API_BASE_URL } from "@plane/constants";
import { Button } from "@makeplane/propel/components/button";
import { Input, InputGroup } from "@makeplane/propel/components/input";
// hooks
import { useUser } from "@/hooks/store";
// components
import { Banner } from "@/components/common/banner";
// local components
import { FormHeader } from "@/components/instance/form-header";
import { AuthBanner } from "./auth-banner";
import { AuthHeader } from "./auth-header";
import { authErrorHandler, EErrorAlertType } from "./auth-helpers";

// error codes
enum EErrorCodes {
  INSTANCE_NOT_CONFIGURED = "INSTANCE_NOT_CONFIGURED",
  REQUIRED_EMAIL_PASSWORD = "REQUIRED_EMAIL_PASSWORD",
  INVALID_EMAIL = "INVALID_EMAIL",
  USER_DOES_NOT_EXIST = "USER_DOES_NOT_EXIST",
  AUTHENTICATION_FAILED = "AUTHENTICATION_FAILED",
}

type TError = {
  type: EErrorCodes | undefined;
  message: string | undefined;
};

// Local fallback for admin error codes outside `EAdminAuthErrorCodes`
// (kept local — `packages/constants` is out of scope): 5000
// INSTANCE_NOT_CONFIGURED + 5021 PASSWORD_TOO_WEAK
// (`authentication/adapter/error.py:7,17`).
const LOCAL_ADMIN_AUTH_FALLBACK: Record<string, TAdminAuthErrorInfo> = {
  "5000": {
    type: EErrorAlertType.BANNER_ALERT,
    code: "5000" as EAdminAuthErrorCodes,
    title: "Instance not configured",
    message: "Instance is not configured. Please complete the setup first.",
  },
  "5021": {
    type: EErrorAlertType.BANNER_ALERT,
    code: "5021" as EAdminAuthErrorCodes,
    title: "Password too weak",
    message: "Password is too weak. Please choose a stronger password.",
  },
};

const localAdminAuthFallback = (code: string): TAdminAuthErrorInfo | undefined => LOCAL_ADMIN_AUTH_FALLBACK[code];

// form data
type TFormData = {
  email: string;
  password: string;
};

const defaultFromData: TFormData = {
  email: "",
  password: "",
};

export function InstanceSignInForm() {
  // router
  const router = useRouter();
  // store hooks
  const { fetchCurrentUser } = useUser();
  // search params
  const searchParams = useSearchParams();
  const emailParam = searchParams.get("email") || undefined;
  const errorCode = searchParams.get("error_code") || undefined;
  const errorMessage = searchParams.get("error_message") || undefined;
  // state
  const [showPassword, setShowPassword] = useState(false);
  const [formData, setFormData] = useState<TFormData>(defaultFromData);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorInfo, setErrorInfo] = useState<TAdminAuthErrorInfo | undefined>(undefined);
  const [submitError, setSubmitError] = useState<string | undefined>(undefined);

  const handleFormChange = (key: keyof TFormData, value: string | boolean) =>
    setFormData((prev) => ({ ...prev, [key]: value }));

  // JSON sign-in (decision B): the Rust backend answers 200 + HttpOnly JWT
  // cookies (`POST /api/instances/admins/sign-in/`) — no token handling in
  // JS. Failures carry `{error_code, error_message}`; known codes reuse the
  // existing admin error display, otherwise the raw message is bannered.
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (isSubmitting) return;
    setIsSubmitting(true);
    setSubmitError(undefined);
    try {
      const res = await fetch(`${API_BASE_URL}/api/instances/admins/sign-in/`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email: formData.email, password: formData.password }),
        credentials: "include",
      });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) {
        const code = data?.error_code !== undefined ? String(data.error_code) : undefined;
        const detail = code
          ? (authErrorHandler(code as EAdminAuthErrorCodes) ?? localAdminAuthFallback(code))
          : undefined;
        if (detail) setErrorInfo(detail);
        else setSubmitError(data?.error_message || "Something went wrong. Please try again.");
        return;
      }
      await fetchCurrentUser().catch(() => undefined);
      router.push("/general");
    } catch {
      setSubmitError("Something went wrong. Please try again.");
    } finally {
      setIsSubmitting(false);
    }
  };

  useEffect(() => {
    if (emailParam) setFormData((prev) => ({ ...prev, email: emailParam }));
  }, [emailParam]);

  // derived values
  const errorData: TError = useMemo(() => {
    if (errorCode && errorMessage) {
      switch (errorCode) {
        case EErrorCodes.INSTANCE_NOT_CONFIGURED:
          return { type: EErrorCodes.INSTANCE_NOT_CONFIGURED, message: errorMessage };
        case EErrorCodes.REQUIRED_EMAIL_PASSWORD:
          return { type: EErrorCodes.REQUIRED_EMAIL_PASSWORD, message: errorMessage };
        case EErrorCodes.INVALID_EMAIL:
          return { type: EErrorCodes.INVALID_EMAIL, message: errorMessage };
        case EErrorCodes.USER_DOES_NOT_EXIST:
          return { type: EErrorCodes.USER_DOES_NOT_EXIST, message: errorMessage };
        case EErrorCodes.AUTHENTICATION_FAILED:
          return { type: EErrorCodes.AUTHENTICATION_FAILED, message: errorMessage };
        default:
          return { type: undefined, message: undefined };
      }
    } else return { type: undefined, message: undefined };
  }, [errorCode, errorMessage]);

  const isButtonDisabled = useMemo(
    () => isSubmitting || !formData.email || !formData.password,
    [formData.email, formData.password, isSubmitting]
  );

  useEffect(() => {
    if (errorCode) {
      const errorDetail =
        authErrorHandler(errorCode?.toString() as EAdminAuthErrorCodes) ?? localAdminAuthFallback(errorCode);
      if (errorDetail) {
        setErrorInfo(errorDetail);
      }
    }
  }, [errorCode]);

  return (
    <>
      <AuthHeader />
      <div className="mt-10 flex w-full flex-grow flex-col items-center justify-center py-6">
        <div className="relative flex w-full max-w-[22.5rem] flex-col gap-6">
          <FormHeader
            heading="Manage your Plane instance"
            subHeading="Configure instance-wide settings to secure your instance"
          />
          <form className="space-y-4" onSubmit={handleSubmit}>
            {errorData.type && errorData?.message ? (
              <Banner type="error" message={errorData?.message} />
            ) : (
              <>{errorInfo && <AuthBanner bannerData={errorInfo} handleBannerData={setErrorInfo} />}</>
            )}
            {submitError && <Banner type="error" message={submitError} />}

            <div className="w-full space-y-1">
              <label className="text-13 font-medium text-tertiary" htmlFor="email">
                Email <span className="text-danger-primary">*</span>
              </label>
              <InputGroup size="lg">
                <Input
                  size="lg"
                  id="email"
                  name="email"
                  type="email"
                  placeholder="name@company.com"
                  value={formData.email}
                  onChange={(e) => handleFormChange("email", e.target.value)}
                  autoComplete="off"
                />
              </InputGroup>
            </div>

            <div className="w-full space-y-1">
              <label className="text-13 font-medium text-tertiary" htmlFor="password">
                Password <span className="text-danger-primary">*</span>
              </label>
              <InputGroup size="lg">
                <Input
                  size="lg"
                  id="password"
                  name="password"
                  type={showPassword ? "text" : "password"}
                  placeholder="Enter your password"
                  value={formData.password}
                  onChange={(e) => handleFormChange("password", e.target.value)}
                  autoComplete="off"
                />
                {showPassword ? (
                  <button
                    type="button"
                    aria-label="Hide password"
                    className="flex items-center justify-center text-placeholder"
                    onClick={() => setShowPassword(false)}
                  >
                    <HideOutline className="h-4 w-4" />
                  </button>
                ) : (
                  <button
                    type="button"
                    aria-label="Show password"
                    className="flex items-center justify-center text-placeholder"
                    onClick={() => setShowPassword(true)}
                  >
                    <ShowOutline className="h-4 w-4" />
                  </button>
                )}
              </InputGroup>
            </div>
            <div className="py-2">
              <Button
                type="submit"
                variant="primary"
                size="lg"
                stretch="full"
                disabled={isButtonDisabled}
                loading={isSubmitting}
                label="Sign in"
              />
            </div>
          </form>
        </div>
      </div>
    </>
  );
}
