# Plane → Terraline Rebrand

Date: 2026-09-18
Status: Approved (pending user review of this spec)
Scope: User-facing rebrand. Internal identifiers, package scope, copyright headers, and
repo/infra names are intentionally left alone.

## Goal

Replace every user-visible trace of the "Plane" brand with "Terraline": logo, logo
animation, product name, marketing copy, metadata, email, colors stay blue, fonts stay
Inter/IBM Plex Mono/Material Symbols. The result should look like a native Terraline
product without destabilising the build.

## Decisions (brainstormed & approved)

1. **Depth** — user-facing only. No rename of `@plane/*` packages, `plane.*` Python
   modules, `apps/api/plane`, 3,814 copyright headers, Docker images
   (`makeplane/plane-*`), systemd units (`plane-*.service`), Caddy `(plane_proxy)`,
   DB/network names, or internal asset filenames.
2. **Logo** — create from scratch. Mark concept "**Strata T**": three rounded, stacked
   horizontal bars (contour / terrace lines) that also read as a capital **T**.
   Single-color via `currentColor`; brand blue `#3f76ff` when colored.
3. **Wordmark** — "Terraline" replacing the "Plane" lettering. Implemented as SVG
   `<text>` with `font-family="Inter Variable, Inter, sans-serif"`, `font-weight=600`,
   tight tracking. App context is safe because Inter is bundled; email context does not
   use the SVG wordmark (see §6).
4. **Animation** — replace the GIF spinner with a dependency-free inline SVG + CSS
   animation on the Terraline mark. Theme-aware via tokens; no separate light/dark GIFs.
5. **Approach** — "A: in-place swap". Keep all existing file names, component names
   (`PlaneLogo`, `PlaneWordmark`, `PlaneLockup`, `LogoSpinner`), and imports. Replace
   their contents and the referenced asset bytes.
6. **Color** — keep blue. `--brand-default` stays `#3f76ff`; `theme_color` stays
   `#3f76ff`.
7. **Typography** — keep `Inter Variable` (heading/body), `IBM Plex Mono` (code),
   `Material Symbols Rounded` (icons). No font-token changes.
8. **Domain / email** — `terraline.space`. Site `https://app.terraline.space/`,
   marketing `https://terraline.space`, support `support@terraline.space`,
   docs `docs.terraline.space`, status `status.terraline.space`.
9. **i18n** — all 20 locales. Value-only replacements, no key/placeholder/plural changes.
10. **External links** — repoint sensible ones to `terraline.space`; remove/hide upstream
    Plane-only links (GitHub `makeplane/plane`, forum, Twitter `@planepowers`, LinkedIn
    `planepowers`, `go.plane.so`, `plane.sh`); blank the default OTLP telemetry endpoint.

## 1. Logo & mark

Files (contents replaced in place):

- `packages/propel/src/icons/brand/plane-logo.tsx` — `PlaneLogo`, mark only. Keep
  `viewBox="0 0 85 52"` and default `width=85 height=52` so layout does not shift.
- `packages/propel/src/icons/brand/plane-wordmark.tsx` — `PlaneWordmark`, "Terraline"
  text.
- `packages/propel/src/icons/brand/plane-lockup.tsx` — `PlaneLockup`, mark + wordmark.
  Keep `viewBox="0 0 253 53"` / `width=253 height=53`.
- `packages/propel/src/icons/sub-brand/plane-icon.tsx` — two-tile sub-brand glyph →
  Terraline strata glyph.
- `apps/admin/components/common/plane-lockup.tsx` — local duplicate, same treatment.
- `packages/propel/public/plane-lockup-light.svg` — static lockup.
- `apps/web/app/assets/plane-logos/*` and `apps/space/app/assets/plane-logos/*` — SVG/PNG
  lockups and mark-only variants.
- `apps/space/app/assets/plane-logo.svg` — base64-embedded mark → Terraline mark.

Mark geometry (single path set, `currentColor`):

- Three horizontally centered rounded bars. The top bar is full-width (the T crossbar);
  the two bars below are progressively narrower, producing a terraced / contour-line
  silhouette.
- Must remain legible at 16px. No fine strokes.

## 2. Animation

Files:

- `apps/web/core/components/common/logo-spinner.tsx`
- `apps/admin/components/common/logo-spinner.tsx`
- `apps/space/components/common/logo-spinner.tsx`

Change: drop the `logo-spinner-dark.gif` / `logo-spinner-light.gif` imports and render an
inline SVG mark with a CSS keyframe animation. Suggested motion: each strata draws /
brightens in sequence bottom→top, then fades, looping smoothly (thematic "terrain lines").

Requirements:

- Keep the `LogoSpinner()` signature (no props) and the existing wrapper sizing
  (`h-6 w-auto object-contain sm:h-11` equivalent).
- Colors resolve from tokens / `currentColor` so light and dark themes work without two
  assets.
- `role="status"`, `aria-label` set.
- `@media (prefers-reduced-motion: reduce)` renders the static mark.
- Delete the six `apps/{web,admin,space}/app/assets/images/logo-spinner-{dark,light}.gif`
  files.

## 3. Static raster assets

Generate from the Terraline SVG mark with **`sharp`** (already present in the pnpm store;
no ImageMagick/rsvg available). A small repo-local script performs SVG→PNG/ICO/WebP
conversion.

Targets:

- `apps/{web,admin,space}/app/assets/favicon/favicon.ico`, `favicon-16x16.png`,
  `favicon-32x32.png`, `apple-touch-icon.png`
- `apps/{web,admin,space}/public/favicon/android-chrome-{192,512}x{192,512}.png`
- `apps/web/app/assets/icons/icon-180x180.png`, `icon-512x512.png`
- `apps/web/public/icons/icon-{192,348,512}x*.png`
- `apps/web/public/plane-logos/plane-mobile-pwa.png`
- `apps/web/app/assets/og-image.png` (mark + "Terraline · Modern project management")
- `apps/web/app/assets/auth/gradient-logo.webp`, `gradient-bg-logo.webp`
- `apps/web/app/assets/plane-takeoff.png`, `apps/admin/app/assets/images/plane-takeoff.png`,
  `apps/space/app/assets/instance/plane-takeoff.png`
- `apps/web/app/assets/instance-not-ready.webp`, `instance-setup-done.webp`,
  `apps/space/app/assets/instance/plane-instance-not-ready.webp`
- `apps/admin/app/assets/logos/takeoff-icon-{light,dark}.svg`

Approach for illustrations: replace airplane artwork with geometric strata/terrain
compositions derived from the mark, authored as SVG source and rasterised via `sharp`.
Internal filenames preserved.

## 4. Text & metadata

- `packages/constants/src/metadata.ts` — `SITE_NAME`, `SITE_TITLE`, `SITE_DESCRIPTION`,
  `SITE_KEYWORDS`, `SITE_URL` (`https://app.terraline.space/`), `TWITTER_USER_NAME`,
  and all `SPACE_*` constants.
- `packages/constants/src/endpoints.ts` — `WEBSITE_URL` (`https://terraline.space`),
  `SUPPORT_EMAIL` (`support@terraline.space`), marketing pricing/contact/one links.
- `packages/constants/src/payment.ts` — plan names "Terraline Pro/Business/Enterprise",
  upgrade and sales URLs on `terraline.space`.
- `apps/web/app/root.tsx`, `apps/admin/app/root.tsx`, `apps/space/app/root.tsx` —
  `APP_TITLE`, `APP_DESCRIPTION`, `application-name`, `og:url`, `og:image:alt`,
  `twitter:*` (remove upstream handle).
- `apps/web/core/components/core/page-title.tsx` — default `document.title`.
- Route title layouts: `sign-up`, `accounts/reset-password`, `accounts/forgot-password`,
  `accounts/set-password`.
- `apps/{web,admin,space}/public/site.webmanifest.json`, `apps/web/public/manifest.json`,
  `apps/web/manifest.json` — name/short_name/description; icons point at regenerated
  assets; `theme_color` stays `#3f76ff`.
- Hardcoded React clusters:
  - `apps/web/core/components/workspace/billing/comparison/plans.tsx` (~16 strings)
  - `apps/web/core/components/onboarding/**` (profile consent, team, usecase, role, tour)
  - `apps/web/core/components/account/auth-forms/**` and `auth-screens/**`
  - `apps/web/core/components/instance/not-ready-view.tsx`,
    `layouts/auth-layout/workspace-wrapper.tsx`, `common/latest-feature-block.tsx`,
    `common/activity/**`, `inbox/**`, `issues/**`, `license/modal/**`
  - `apps/admin/app/(all)/**` setup/oauth/ai/email/general/help/sidebar forms
  - `apps/space/components/common/powered-by.tsx`,
    `components/account/auth-forms/auth-header.tsx`, `app/error.tsx`,
    `lib/instance-provider.tsx`, `components/instance/instance-failure-view.tsx`

## 5. i18n

- `packages/i18n/src/locales/**` (20 locales): value-only replacement of `Plane` →
  `Terraline` and `plane.so` → `terraline.space`. Do not touch keys, placeholders, or
  plural forms.
- Branded keys to review individually: `Plane AI` → `Terraline AI`, `Plane Pro` →
  `Terraline Pro`, `Plane Pages`, `Plane Runner`, `Plane-only feature`.
- Backend seed content mirrors the same text: `apps/api/plane/seeds/data/projects.json`,
  `issues.json`, `pages.json`, `cycles.json`.

## 6. Email & backend (user-facing)

- 12 templates under `apps/api/templates/emails/**`: replace "Plane" → "Terraline",
  footer "Plane Software, Inc." → "Terraline"; remove remote
  `media.docs.plane.so/logo/*` and `plane-marketing.s3.../plane-assets/*` image URLs
  (replace with mark + HTML text header, sans-serif fallback); point links at
  `terraline.space`.
- Subject lines in `apps/api/plane/bgtasks/{forgot_password,magic_link_code,
  user_activation_email,user_deactivation_email,user_email_update,project_add_user_email,
  project_invitation,workspace_invitation}_task.py`.
- `apps/api/plane/settings/openapi.py` — title/description/contact/license/servers.
- `apps/api/plane/settings/production.py` — `SCOUT_NAME`.
- `apps/api/plane/license/utils/instance_value.py` — default email sender
  `Team Terraline <team@terraline.space>`.
- `apps/api/plane/license/management/commands/register_instance.py` — instance name.
- `apps/api/templates/base.html`, `apps/api/templates/admin/base_site.html`.
- `apps/api/plane/static/logos/Logo.png` → Terraline wordmark.
- `apps/api/Dockerfile.api`, `apps/api/Dockerfile.dev` — `INSTANCE_CHANGELOG_URL`.

## 7. External links

- Repoint: `docs.plane.so` → `docs.terraline.space`, `developers.plane.so` →
  `developers.terraline.space`, `status.plane.so` → `status.terraline.space`,
  `support@`/`sales@`/`security@`/`squawk@` plane.so → `terraline.space`,
  `plane.so/*` → `terraline.space/*`, `app.plane.so` → `app.terraline.space`,
  `sites.plane.so` → `sites.terraline.space`.
- Remove/hide: `github.com/makeplane/plane`, `forum.plane.so`,
  `x.com/planepowers`, `linkedin.com/company/planepowers`, `go.plane.so`, `plane.sh`,
  and the Plane social icons in email footers.
- `apps/api/plane/utils/otlp_endpoints.py` — default OTLP endpoint blanked.
- Call sites include `apps/web/app/error/prod.tsx`,
  `apps/web/core/components/workspace/sidebar/help-section/root.tsx`,
  `apps/web/core/components/global/product-updates/{footer,fallback}.tsx`,
  `apps/web/core/components/power-k/config/help-commands.ts`,
  `apps/web/app/(all)/[workspaceSlug]/(projects)/star-us-link.tsx`, and the
  `apps/web/core/components/account/terms-and-conditions.tsx` legal links.

## Out of scope

- `@plane/*` package scope, `plane.*` Python modules, `apps/api/plane` directory name.
- Copyright / SPDX headers (3,814 files).
- Docker images, systemd, Caddy, DB/network names, `deployments/**`.
- README / CONTRIBUTING / SECURITY / docs prose (repo-facing, not product UI).
- Any backend behavior change beyond strings.

## Verification

1. `pnpm check` (format, lint, types) passes.
2. `pnpm --filter=web build` succeeds; per AGENTS.md rebuild + restart
   `plane-web-prod.service` for the tunnel.
3. Grep audit for residual user-facing `Plane` / `plane.so` outside identifiers,
   copyright headers, package scope, and repo docs.
4. Manual smoke on dev/prod: login screen, loading spinner, web app chrome, admin
   dashboard, space publish page, one email template render.
5. Confirm regenerated favicon/PWA/OG assets are valid images at expected dimensions.
6. Confirm `prefers-reduced-motion` renders a static logo.

## Key trade-offs

- Wordmark uses SVG `<text>` (Inter-dependent) rather than vectorised font paths; email
  avoids this by using HTML text.
- Illustrations are replaced with geometric strata artwork rather than a bespoke
  character illustration; keeps raster generation scriptable via `sharp`.
- Internal `plane-*` identifiers remain, per the user-facing-only scope.
