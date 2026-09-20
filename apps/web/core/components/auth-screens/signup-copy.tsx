/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";

const TRUST_KEYS = ["auth.sign_up.copy.trust_1", "auth.sign_up.copy.trust_2", "auth.sign_up.copy.trust_3"] as const;

type SignupCopyVariant = "light" | "dark";

export type SignupCopyKind = "signup" | "signin";

const COPY_PREFIX: Record<SignupCopyKind, string> = {
  signup: "auth.sign_up.copy",
  signin: "auth.sign_in.copy",
};

export function AuthSignupCopy({
  variant = "light",
  kind = "signup",
}: {
  variant?: SignupCopyVariant;
  kind?: SignupCopyKind;
}) {
  const { t } = useTranslation();
  const dark = variant === "dark";
  const prefix = COPY_PREFIX[kind];
  return (
    <div className={dark ? "relative" : "relative mt-8 mb-6"}>
      {!dark && (
        <div
          aria-hidden="true"
          className="auth-dotgrid pointer-events-none absolute -top-8 -right-6 -left-6 h-64 sm:-right-8 sm:-left-8"
        />
      )}
      <div className="auth-rise relative flex items-center gap-3" style={{ animationDelay: "0ms" }}>
        <span
          aria-hidden="true"
          className={dark ? "size-1.5 shrink-0 bg-white/70" : "size-1.5 shrink-0 bg-accent-primary"}
        />
        <span
          className={
            dark
              ? "font-code text-11 font-medium tracking-[0.18em] text-white/60 uppercase"
              : "font-code text-11 font-medium tracking-[0.18em] text-accent-primary uppercase"
          }
        >
          {t(`${prefix}.eyebrow`)}
        </span>
        <span aria-hidden="true" className={dark ? "h-px flex-1 bg-white/15" : "bg-border-strong/40 h-px flex-1"} />
      </div>
      <h1
        className={
          dark
            ? "auth-rise relative mt-5 max-w-[20ch] text-[clamp(2.1rem,1.6rem+2.4vw,3.25rem)] leading-[1.04] font-bold tracking-[-0.02em] text-balance text-white"
            : "auth-rise relative mt-5 max-w-[18ch] text-[clamp(2rem,1.4rem+2.2vw,2.75rem)] leading-[1.05] font-bold tracking-[-0.02em] text-balance text-primary"
        }
        style={{ animationDelay: "90ms" }}
      >
        {t(`${prefix}.headline`)}
      </h1>
      <p
        className={
          dark
            ? "auth-rise relative mt-4 max-w-[46ch] text-[15px] leading-relaxed text-white/60"
            : "auth-rise relative mt-4 max-w-[42ch] text-[15px] leading-relaxed text-tertiary"
        }
        style={{ animationDelay: "180ms" }}
      >
        {t(`${prefix}.subcopy`)}
      </p>
    </div>
  );
}

export function AuthSignupTrust({ variant = "light" }: { variant?: SignupCopyVariant }) {
  const { t } = useTranslation();
  const dark = variant === "dark";
  return (
    <ol className="auth-rise mt-6 list-none" style={{ animationDelay: "260ms" }}>
      {TRUST_KEYS.map((key, index) => (
        <li
          key={key}
          className={
            dark
              ? "flex items-baseline gap-4 border-t border-white/10 py-2.5 last:border-b"
              : "border-border-strong/40 flex items-baseline gap-4 border-t py-2.5 last:border-b"
          }
        >
          <span
            aria-hidden="true"
            className={dark ? "font-code text-11 text-white/30" : "font-code text-11 text-placeholder"}
          >
            {String(index + 1).padStart(2, "0")}
          </span>
          <span className={dark ? "text-[13px] font-medium text-white/70" : "text-[13px] font-medium text-secondary"}>
            {t(key)}
          </span>
        </li>
      ))}
    </ol>
  );
}
