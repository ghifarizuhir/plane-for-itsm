/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";

const TRUST_KEYS = ["auth.sign_up.copy.trust_1", "auth.sign_up.copy.trust_2", "auth.sign_up.copy.trust_3"] as const;

export function AuthSignupCopy() {
  const { t } = useTranslation();
  return (
    <div className="relative mt-10 mb-8">
      <div aria-hidden="true" className="auth-dotgrid pointer-events-none absolute -top-10 -right-8 -left-8 h-64" />
      <div className="auth-rise relative flex items-center gap-3" style={{ animationDelay: "0ms" }}>
        <span aria-hidden="true" className="size-1.5 shrink-0 bg-accent-primary" />
        <span className="font-code text-11 font-medium tracking-[0.18em] text-accent-primary uppercase">
          {t("auth.sign_up.copy.eyebrow")}
        </span>
        <span aria-hidden="true" className="bg-border-strong/40 h-px flex-1" />
      </div>
      <h1
        className="auth-rise relative mt-5 max-w-[16ch] text-[2.75rem] leading-[1.04] font-bold tracking-[-0.02em] text-balance text-primary"
        style={{ animationDelay: "90ms" }}
      >
        {t("auth.sign_up.copy.headline")}
      </h1>
      <p
        className="auth-rise relative mt-4 max-w-[42ch] text-[15px] leading-relaxed text-tertiary"
        style={{ animationDelay: "180ms" }}
      >
        {t("auth.sign_up.copy.subcopy")}
      </p>
    </div>
  );
}

export function AuthSignupTrust() {
  const { t } = useTranslation();
  return (
    <ol className="auth-rise mt-8 list-none" style={{ animationDelay: "260ms" }}>
      {TRUST_KEYS.map((key, index) => (
        <li key={key} className="border-border-strong/40 flex items-baseline gap-4 border-t py-2.5 last:border-b">
          <span aria-hidden="true" className="font-code text-11 text-placeholder">
            {String(index + 1).padStart(2, "0")}
          </span>
          <span className="text-[13px] font-medium text-secondary">{t(key)}</span>
        </li>
      ))}
    </ol>
  );
}
