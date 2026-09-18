#!/usr/bin/env node
// scripts/terraline-assets.mjs
import { mkdirSync, writeFileSync, copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const BRAND = "#3F76FF";

// Point fontconfig at the in-repo Inter TTFs so librsvg can render the wordmark.
process.env.FONTCONFIG_FILE = "/tmp/opencode/terraline-fonts.conf";
mkdirSync("/tmp/opencode/terraline-fontconfig-cache", { recursive: true });
writeFileSync(
  process.env.FONTCONFIG_FILE,
  `<?xml version="1.0"?>\n<!DOCTYPE fontconfig SYSTEM "fonts.dtd">\n<fontconfig>\n  <dir>${join(root, "apps/web/app/assets/fonts/inter")}</dir>\n  <cachedir>/tmp/opencode/terraline-fontconfig-cache</cachedir>\n</fontconfig>\n`,
);

const { default: sharp } = await import("sharp");

const mark = (fill = BRAND, scale = 1) => {
  const w = Math.round(85 * scale);
  const h = Math.round(52 * scale);
  const r = 7 * scale;
  const x = (n) => n * scale;
  return `<svg width="${w}" height="${h}" viewBox="0 0 85 52" xmlns="http://www.w3.org/2000/svg">
    <rect x="0" y="0" width="85" height="14" rx="7" fill="${fill}"/>
    <rect x="10.625" y="19" width="63.75" height="14" rx="7" fill="${fill}"/>
    <rect x="21.25" y="38" width="42.5" height="14" rx="7" fill="${fill}"/>
  </svg>`;
};

const lockup = (fill, bg) => `<svg width="506" height="106" viewBox="0 0 253 53" xmlns="http://www.w3.org/2000/svg">
  ${bg ? `<rect width="253" height="53" fill="${bg}"/>` : ""}
  <rect x="0" y="0.5" width="85" height="14" rx="7" fill="${fill}"/>
  <rect x="10.625" y="19.5" width="63.75" height="14" rx="7" fill="${fill}"/>
  <rect x="21.25" y="38.5" width="42.5" height="14" rx="7" fill="${fill}"/>
  <text x="96" y="38" fill="${fill}" font-family="Inter, DejaVu Sans, sans-serif" font-size="36" font-weight="600" letter-spacing="-0.02em" textLength="152" lengthAdjust="spacingAndGlyphs">Terraline</text>
</svg>`;

async function png(svg, size, out) {
  mkdirSync(dirname(out), { recursive: true });
  await sharp(Buffer.from(svg), { density: 384 })
    .resize(size, size, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 0 } })
    .png()
    .toFile(out);
  console.log(`wrote ${out}`);
}

async function lockupPng(fill, bg, out) {
  mkdirSync(dirname(out), { recursive: true });
  await sharp(Buffer.from(lockup(fill, bg)), { density: 384 }).png().toFile(out);
  console.log(`wrote ${out}`);
}

// Mark-only icons on brand background
const iconTile = `<svg width="512" height="512" viewBox="0 0 512 512" xmlns="http://www.w3.org/2000/svg">
  <rect width="512" height="512" rx="112" fill="${BRAND}"/>
  <g transform="translate(106 164) scale(3.53)">${mark("#FFFFFF").replace(/<\/?svg[^>]*>/g, "")}</g>
</svg>`;

const targets = [
  [iconTile, 16, "apps/web/app/assets/favicon/favicon-16x16.png"],
  [iconTile, 32, "apps/web/app/assets/favicon/favicon-32x32.png"],
  [iconTile, 180, "apps/web/app/assets/favicon/apple-touch-icon.png"],
  [iconTile, 180, "apps/web/app/assets/icons/icon-180x180.png"],
  [iconTile, 512, "apps/web/app/assets/icons/icon-512x512.png"],
  [iconTile, 192, "apps/web/public/favicon/android-chrome-192x192.png"],
  [iconTile, 512, "apps/web/public/favicon/android-chrome-512x512.png"],
  [iconTile, 192, "apps/web/public/icons/icon-192x192.png"],
  [iconTile, 348, "apps/web/public/icons/icon-348x348.png"],
  [iconTile, 512, "apps/web/public/icons/icon-512x512.png"],
  [iconTile, 512, "apps/web/public/plane-logos/plane-mobile-pwa.png"],
  [iconTile, 192, "apps/admin/public/favicon/android-chrome-192x192.png"],
  [iconTile, 512, "apps/admin/public/favicon/android-chrome-512x512.png"],
  [iconTile, 16, "apps/admin/app/assets/favicon/favicon-16x16.png"],
  [iconTile, 32, "apps/admin/app/assets/favicon/favicon-32x32.png"],
  [iconTile, 180, "apps/admin/app/assets/favicon/apple-touch-icon.png"],
  [iconTile, 192, "apps/space/public/favicon/android-chrome-192x192.png"],
  [iconTile, 512, "apps/space/public/favicon/android-chrome-512x512.png"],
  [iconTile, 16, "apps/space/app/assets/favicon/favicon-16x16.png"],
  [iconTile, 32, "apps/space/app/assets/favicon/favicon-32x32.png"],
  [iconTile, 180, "apps/space/app/assets/favicon/apple-touch-icon.png"],
];
for (const [svg, size, out] of targets) await png(svg, size, join(root, out));

// OG image
const og = `<svg width="1201" height="631" viewBox="0 0 1201 631" xmlns="http://www.w3.org/2000/svg">
  <rect width="1201" height="631" fill="#0A0A0A"/>
  <g transform="translate(96 250) scale(1.6)">${mark(BRAND).replace(/<\/?svg[^>]*>/g, "")}</g>
  <text x="248" y="360" fill="#FFFFFF" font-family="Inter, DejaVu Sans, sans-serif" font-size="120" font-weight="600" letter-spacing="-0.03em">Terraline</text>
  <text x="252" y="420" fill="#9CA3AF" font-family="Inter, DejaVu Sans, sans-serif" font-size="40">Modern work management</text>
</svg>`;
await sharp(Buffer.from(og), { density: 192 }).resize(1201, 631).png().toFile(join(root, "apps/web/app/assets/og-image.png"));
console.log("wrote apps/web/app/assets/og-image.png");

// Gradient auth logos (webp)
const gradient = `<svg width="512" height="512" viewBox="0 0 512 512" xmlns="http://www.w3.org/2000/svg">
  <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="#3F76FF"/><stop offset="1" stop-color="#05C3FF"/>
  </linearGradient></defs>
  <g transform="translate(106 164) scale(3.53)">${mark("url(#g)").replace(/<\/?svg[^>]*>/g, "")}</g>
</svg>`;
for (const out of [
  "apps/web/app/assets/auth/gradient-logo.webp",
  "apps/web/app/assets/auth/gradient-bg-logo.webp",
]) {
  await sharp(Buffer.from(gradient), { density: 384 }).webp().toFile(join(root, out));
  console.log(`wrote ${out}`);
}

// Full lockup PNG variants used by app/space asset folders
await lockupPng("#0A0A0A", null, join(root, "apps/web/app/assets/plane-logos/black-horizontal-with-blue-logo.png"));
await lockupPng("#FFFFFF", null, join(root, "apps/web/app/assets/plane-logos/white-horizontal-with-blue-logo.png"));
await lockupPng("#0A0A0A", null, join(root, "apps/space/app/assets/plane-logos/black-horizontal-with-blue-logo.png"));
await lockupPng("#FFFFFF", null, join(root, "apps/space/app/assets/plane-logos/white-horizontal-with-blue-logo.png"));
await lockupPng("#FFFFFF", null, join(root, "apps/api/plane/static/logos/Logo.png"));
for (const out of [
  "apps/web/app/assets/plane-logos/blue-without-text.png",
  "apps/space/app/assets/plane-logos/blue-without-text.png",
  "apps/space/app/assets/plane-logos/blue-without-text-new.png",
]) await png(mark(BRAND), 512, join(root, out));

// .ico (largest frame; acceptable for modern browsers)
copyFileSync(
  join(root, "apps/web/app/assets/favicon/favicon-32x32.png"),
  join(root, "apps/web/app/assets/favicon/favicon.ico"),
);
copyFileSync(
  join(root, "apps/web/app/assets/favicon/favicon-32x32.png"),
  join(root, "apps/admin/app/assets/favicon/favicon.ico"),
);
copyFileSync(
  join(root, "apps/web/app/assets/favicon/favicon-32x32.png"),
  join(root, "apps/space/app/assets/favicon/favicon.ico"),
);
console.log("favicon.ico files updated");
