# Auth Sign-up Left Panel Copy Redesign — Design Spec

Date: 2026-09-20
Status: Approved (Sections 1–2)
Scope: `apps/web` sign-up left panel only

## 1. Context

`AuthBase` (`apps/web/core/components/auth-screens/auth-base.tsx`) renders a two-column
auth screen on desktop:

- Left (~46%): `AuthHeader` (Terraline lockup + sign-in link) + `AuthRoot` (form).
- Right (flex-1, desktop only): `AuthWavesPanel` (interactive waves, "Terraline ITSM",
  "Work in all dimensions").

The marketing footer (`AuthFooter` — "Join 10,000+ teams" + brand logos) was removed
previously. The left column currently has no product copy — it jumps straight from
header into the form.

Goal: add compact, human + professional copywriting to the left panel on sign-up,
with a self-hosted / control angle.

## 2. Decisions

| Question        | Answer                                                                                                    |
| --------------- | --------------------------------------------------------------------------------------------------------- |
| Which panel     | Left form panel (not the right waves visual, no swap)                                                     |
| Copy angle      | Self-hosted / control (data ownership, host anywhere)                                                     |
| Layout density  | Compact header: headline + 1-line subcopy above form, trust line below                                    |
| Final direction | C2 — warm + human (user picked C, then C2 over C3; clicked B once in browser but confirmed C in terminal) |
| Scope           | `SIGN_UP` only; `SIGN_IN` unchanged                                                                       |
| Right panel     | Untouched                                                                                                 |

Options considered: A ("Your infrastructure. Your rules."), B ("Own your ITSM stack,
end to end."), C ("Enterprise ITSM without the lock-in."), then C2 vs C3 refinement.
C2 won for warmth without losing enterprise credibility and for dropping the
negative "lock-in" wording.

## 3. Final copy (C2, English)

- Eyebrow: `Built for IT teams`
- H1: `Take care of IT, without giving up control.`
- Subcopy: `Terraline brings requests, approvals, and audits together in one calm place — hosted your way, with your data staying yours.`
- Trust line (below form): `Self-host or cloud • Your data stays yours • Audit-ready`

Placement (desktop + mobile stacked):

1. `AuthHeader` (unchanged)
2. `AuthSignupCopy` — eyebrow + H1 + subcopy
3. `AuthRoot` (existing form, unchanged)
4. Trust line — centered, tertiary, below form

Mockups: `.superpowers/brainstorm/645498-1789871029/content/copy-c-v2.html` (C2 card).

## 4. Technical design

### 4.1 Components

- New file: `apps/web/core/components/auth-screens/signup-copy.tsx`
  - `AuthSignupCopy()` — eyebrow, H1, subcopy. No props (sign-up only).
  - `AuthSignupTrust()` (or a `variant` prop) — trust line below the form.
  - Single-purpose, no form logic, no state.
- Edit: `apps/web/core/components/auth-screens/auth-base.tsx`
  - Conditionally render copy + trust only when `authType === EAuthModes.SIGN_UP`.
  - `SIGN_IN` path renders exactly as today.

### 4.2 i18n

- New keys under `auth.sign_up.copy` in `packages/i18n/src/locales/en/auth.json`:
  `eyebrow`, `headline`, `subcopy`, `trust`.
- Component reads via existing `useTranslation()` pattern (see `header.tsx`).
- Other locales fall back to English until translators catch up; no blocking
  per-locale work in this change.

### 4.3 Styling / responsive

- Tailwind only, reusing existing tokens (`text-tertiary`, body/sm scales).
- Eyebrow: small uppercase tracking, accent color.
- H1: ~text-2xl/extrabold, tight leading; subcopy: text-sm/relaxed tertiary.
- Vertical rhythm: copy block `mb-6`, trust line `mt-6` centered.
- Mobile (`<lg`): copy stacks above form naturally; no horizontal changes.
- No width changes to the 46% column; no waves-panel changes; dark-mode safe
  (inherit current tokens).

### 4.4 States / errors

- No new loading, error, or empty states. Copy is static.
- If translation keys are missing, fall back to the English strings above.

## 5. Out of scope

- Right `AuthWavesPanel` changes (text, colors, behavior).
- `SIGN_IN` copy.
- Form fields, validation, auth logic.
- Brand logos / social proof footer (stays removed).
- Full per-locale translations.

## 6. Verification

- `pnpm --filter=web exec tsc --noEmit`
- `pnpm exec oxlint apps/web/core/components/auth-screens/`
- `pnpm --filter=web build` + restart `plane-web-prod.service` for tunnel demo.
- Visual check: `/sign-up` desktop + mobile widths; confirm sign-in unchanged.

## 7. Risks

- Narrow column + long H1 wrapping — mitigated by compact two-line headline and
  existing column min-widths (`30rem`/`34rem`).
- i18n string length variance — subcopy kept to one sentence; layout tolerates
  2–3 lines.
