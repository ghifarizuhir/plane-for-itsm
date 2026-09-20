/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";

export function AuthSignupCopy() {
  const { t } = useTranslation();
  return (
    <div className="mt-8 mb-6 flex flex-col gap-2">
      <span className="text-11 font-semibold tracking-[0.12em] text-accent-primary uppercase">
        {t("auth.sign_up.copy.eyebrow")}
      </span>
      <h1 className="text-2xl font-extrabold leading-tight text-primary">{t("auth.sign_up.copy.headline")}</h1>
      <p className="text-sm leading-relaxed text-tertiary">{t("auth.sign_up.copy.subcopy")}</p>
    </div>
  );
}

export function AuthSignupTrust() {
  const { t } = useTranslation();
  return <p className="mt-6 text-center text-12 text-tertiary">{t("auth.sign_up.copy.trust")}</p>;
}
