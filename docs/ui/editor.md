# Editor — TipTap + Yjs

Status: **Draft.**

File: `packages/editor` + `apps/live` (Hocuspocus). Adaptasi dari `terra/docs/ui/editor.md:1` — Terra pakai TipTap standalone; Plane pakai `packages/editor` + `apps/live` Yjs collaboration.

---

## Stack

- **TipTap 2** (`@tiptap/core`, `@tiptap/starter-kit`, `@tiptap/extension-mention/image/placeholder` — `pnpm-workspace.yaml:52`) — `packages/editor`.
- **Yjs** (`yjs 13.6`, `y-prosemirror 1.3`, `y-indexeddb 9.0` — `pnpm-workspace.yaml:188`) — CRDT.
- **Hocuspocus** (`@hocuspocus/server 2.15`, `@hocuspocus/provider 2.15`, `extension-redis/database` — `pnpm-workspace.yaml:23`) — `apps/live` (Express+ws).
- `prosemirror-view 1.40`, `lowlight 3.0` (syntax), `highlight.js` — code blocks.

---

## Usage

### Read/Write

- **Write:** TipTap `Editor` di `apps/web` issue/page description — toolbar (bold/italic/list/code/mention/image), slash menu, BubbleMenu.
- **Read:** `MarkdownContent` (marked + sanitize + highlight + entity-mention pills).
- **Collab Pages:** Hocuspocus provider sync via `apps/live` — `Y.Doc` + `y-prosemirror` binding; cursor presence + offline `y-indexeddb`.

### Mention (`@`)

- `@` → `Mention` extension (`@tiptap/extension-mention:61`) — query `searchIssues` / `searchUsers` (actual).
- Render pill: `bg-wash-2 rounded-full px-2 text-xs font-medium` (mirip Terra `.bn-entity-mention`).

### Code Block

- `lowlight` + `highlight.js` — lang label + copy button persistent (bukan hover-reveal).
- Server render tetap dark (`code-shell` tokens) di kedua theme.

---

## Config

- Content stored sebagai **TipTap JSON** di `Issue.description` (`JSONField` — `plane/db/models/issue.py`) — bukan markdown file.
- Page collaboration: `Document` + `Paragraph` + `Heading` + `List` + `Mention` extensions; schema di `packages/editor/src/extensions`.

---

---

## Changelog

| Date       | Change |
| ---------- | ------ |
| 2026-09-03 | —      |
