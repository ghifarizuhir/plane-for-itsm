/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import {
  AlignmentType,
  ExternalHyperlink,
  HeadingLevel,
  ImageRun,
  LevelFormat,
  Paragraph,
  ShadingType,
  Table,
  TableCell,
  TableRow,
  TextRun,
  WidthType,
} from "docx";
import { getBase64Image } from "@plane/utils";

export const MAX_IMAGE_WIDTH = 600;
export const DEFAULT_IMAGE_UNAVAILABLE_TEXT = "Image unavailable";
export const DOCX_BULLET_REFERENCE = "doc-export-bullet";
export const DOCX_NUMBERED_REFERENCE = "doc-export-numbered";

export type TDocumentBlock = Paragraph | Table;

export type TDocxImage = {
  data: Uint8Array;
  width: number;
  height: number;
  type: "png" | "jpg" | "gif";
};

export type TImageFetcher = (src: string) => Promise<TDocxImage | null>;

export type THtmlToDocxOptions = {
  imageFetcher?: TImageFetcher;
  maxImageWidth?: number;
  imageUnavailableText?: string;
};

const HEADING_BY_TAG: Record<string, (typeof HeadingLevel)[keyof typeof HeadingLevel]> = {
  h1: HeadingLevel.HEADING_1,
  h2: HeadingLevel.HEADING_2,
  h3: HeadingLevel.HEADING_3,
  h4: HeadingLevel.HEADING_4,
  h5: HeadingLevel.HEADING_5,
  h6: HeadingLevel.HEADING_6,
};

const BORDER = { style: "single" as const, size: 4, color: "D9D9D9" };

type TRunStyle = {
  bold?: boolean;
  italics?: boolean;
  underline?: boolean;
  strike?: boolean;
  code?: boolean;
  link?: boolean;
};

const isElement = (node: Node): node is HTMLElement => node.nodeType === 1;

const dataUrlToUint8Array = (dataUrl: string): Uint8Array => {
  const binary = atob(dataUrl.split(",")[1] ?? "");
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
};

const defaultImageFetcher: TImageFetcher = async (src) => {
  try {
    const dataUrl = await getBase64Image(src);
    const mime = dataUrl.slice(5, dataUrl.indexOf(";"));
    if (mime === "image/svg+xml") return null;
    const type = mime === "image/jpeg" ? "jpg" : mime === "image/gif" ? "gif" : "png";
    const data = dataUrlToUint8Array(dataUrl);
    const bitmap = await createImageBitmap(await (await fetch(dataUrl)).blob());
    return { data, width: bitmap.width, height: bitmap.height, type };
  } catch {
    return null;
  }
};

const scaledSize = (image: TDocxImage, maxWidth: number, widthHint?: number) => {
  const targetWidth = Math.min(widthHint ?? image.width, maxWidth);
  if (targetWidth === image.width) return { width: image.width, height: image.height };
  const ratio = targetWidth / image.width;
  return { width: targetWidth, height: Math.round(image.height * ratio) };
};

const isImageElement = (element: Element) => ["img", "image-component"].includes(element.tagName.toLowerCase());

const pxWidthHint = (element: Element): number | undefined => {
  const match = element.getAttribute("width")?.match(/^(\d+)px$/);
  return match ? Number(match[1]) : undefined;
};

const isMention = (element: HTMLElement) =>
  element.getAttribute("data-type") === "mention" || element.classList.contains("mention");

const collectRuns = (node: Node, style: TRunStyle): (TextRun | ExternalHyperlink)[] => {
  if (node.nodeType === 3) {
    const text = node.textContent ?? "";
    if (!text) return [];
    return [
      new TextRun({
        text,
        bold: style.bold,
        italics: style.italics,
        underline: style.underline || style.link ? {} : undefined,
        strike: style.strike,
        font: style.code ? "Courier New" : undefined,
        color: style.link ? "0563C1" : undefined,
      }),
    ];
  }
  if (!isElement(node)) return [];
  const element = node;
  const tag = element.tagName.toLowerCase();
  if (tag === "br") return [new TextRun({ break: 1 })];
  if (tag === "script" || tag === "style" || isImageElement(element)) return [];
  if (isMention(element)) {
    const label = element.getAttribute("data-label") ?? element.textContent ?? "";
    return [new TextRun({ text: `@${label.trim()}` })];
  }
  const next: TRunStyle = { ...style };
  if (tag === "strong" || tag === "b") next.bold = true;
  if (tag === "em" || tag === "i") next.italics = true;
  if (tag === "u") next.underline = true;
  if (tag === "s" || tag === "strike" || tag === "del") next.strike = true;
  if (tag === "code") next.code = true;
  if (tag === "a") {
    next.link = true;
    const href = element.getAttribute("href");
    const children = Array.from(element.childNodes).flatMap((child) => collectRuns(child, next));
    const runs = children.filter((child): child is TextRun => child instanceof TextRun);
    if (href && runs.length) return [new ExternalHyperlink({ link: href, children: runs })];
    return children;
  }
  return Array.from(element.childNodes).flatMap((child) => collectRuns(child, next));
};

type TWalkContext = { imageFetcher: TImageFetcher; maxImageWidth: number; imageUnavailableText: string };

const imageParagraph = async (element: Element, ctx: TWalkContext): Promise<Paragraph> => {
  const image = await ctx.imageFetcher(element.getAttribute("src") ?? "");
  if (!image) {
    const alt = element.getAttribute("alt") ?? "";
    return new Paragraph({
      children: [new TextRun({ text: `[${alt || ctx.imageUnavailableText}]`, italics: true, color: "808080" })],
      spacing: { after: 120 },
    });
  }
  return new Paragraph({
    children: [
      new ImageRun({
        type: image.type,
        data: image.data,
        transformation: scaledSize(image, ctx.maxImageWidth, pxWidthHint(element)),
      }),
    ],
    spacing: { before: 120, after: 120 },
  });
};

const listBlocks = async (list: HTMLElement, level: number, ctx: TWalkContext): Promise<Paragraph[]> => {
  const ordered = list.tagName.toLowerCase() === "ol";
  const blocks: Paragraph[] = [];
  const items = Array.from(list.children).filter((child) => child.tagName.toLowerCase() === "li");
  for (const item of items) {
    const nested: HTMLElement[] = [];
    const runs: (TextRun | ExternalHyperlink)[] = [];
    for (const child of Array.from(item.childNodes)) {
      if (isElement(child) && ["ul", "ol"].includes(child.tagName.toLowerCase())) {
        nested.push(child);
        continue;
      }
      runs.push(...collectRuns(child, {}));
    }
    blocks.push(
      new Paragraph({
        numbering: {
          reference: ordered ? DOCX_NUMBERED_REFERENCE : DOCX_BULLET_REFERENCE,
          level: Math.min(level, 2),
        },
        children: runs,
        spacing: { after: 40 },
      })
    );
    for (const image of Array.from(item.querySelectorAll("img, image-component"))) {
      // oxlint-disable-next-line no-await-in-loop -- images must keep DOM order
      blocks.push(await imageParagraph(image, ctx));
    }
    // oxlint-disable-next-line no-await-in-loop -- nested items must keep DOM order
    for (const child of nested) blocks.push(...(await listBlocks(child, level + 1, ctx)));
  }
  return blocks;
};

const tableBlock = (table: HTMLElement): Table => {
  const rows = Array.from(table.querySelectorAll("tr")).map((row) => {
    const cells = Array.from(row.querySelectorAll("th,td"));
    return new TableRow({
      children: cells.map((cell) => {
        const isHeader = cell.tagName.toLowerCase() === "th";
        return new TableCell({
          shading: isHeader ? { type: ShadingType.CLEAR, fill: "F2F2F2", color: "auto" } : undefined,
          children: [new Paragraph({ children: collectRuns(cell, { bold: isHeader }) })],
        });
      }),
    });
  });
  return new Table({
    rows,
    width: { size: 100, type: WidthType.PERCENTAGE },
    borders: {
      top: BORDER,
      bottom: BORDER,
      left: BORDER,
      right: BORDER,
      insideHorizontal: BORDER,
      insideVertical: BORDER,
    },
  });
};

async function walkBlocks(nodes: Node[], ctx: TWalkContext): Promise<TDocumentBlock[]> {
  const blocks: TDocumentBlock[] = [];
  for (const node of nodes) {
    if (node.nodeType === 3) {
      const text = node.textContent?.trim();
      if (text) blocks.push(new Paragraph({ children: [new TextRun({ text })], spacing: { after: 120 } }));
      continue;
    }
    if (!isElement(node)) continue;
    const element = node;
    const tag = element.tagName.toLowerCase();

    if (HEADING_BY_TAG[tag]) {
      blocks.push(new Paragraph({ heading: HEADING_BY_TAG[tag], children: collectRuns(element, {}) }));
      continue;
    }
    if (tag === "p") {
      const images = Array.from(element.querySelectorAll("img, image-component"));
      for (const image of images) {
        // oxlint-disable-next-line no-await-in-loop -- blocks must keep DOM order
        blocks.push(await imageParagraph(image, ctx));
      }
      const runs = images.length
        ? Array.from(element.childNodes).flatMap((child) =>
            isElement(child) && isImageElement(child) ? [] : collectRuns(child, {})
          )
        : collectRuns(element, {});
      if (runs.length) blocks.push(new Paragraph({ children: runs, spacing: { after: 120 } }));
      continue;
    }
    if (isImageElement(element)) {
      // oxlint-disable-next-line no-await-in-loop -- blocks must keep DOM order
      blocks.push(await imageParagraph(element, ctx));
      continue;
    }
    if (tag === "ul" || tag === "ol") {
      // oxlint-disable-next-line no-await-in-loop -- blocks must keep DOM order
      blocks.push(...(await listBlocks(element, 0, ctx)));
      continue;
    }
    if (tag === "blockquote") {
      blocks.push(
        new Paragraph({
          children: collectRuns(element, { italics: true }),
          indent: { left: 720 },
          spacing: { after: 120 },
        })
      );
      continue;
    }
    if (tag === "pre") {
      blocks.push(
        new Paragraph({
          children: [new TextRun({ text: element.textContent ?? "", font: "Courier New", size: 20 })],
          shading: { type: ShadingType.CLEAR, fill: "F5F5F5", color: "auto" },
          spacing: { before: 120, after: 120 },
        })
      );
      continue;
    }
    if (tag === "table") {
      blocks.push(tableBlock(element));
      continue;
    }
    if (tag === "hr") {
      blocks.push(
        new Paragraph({
          border: { bottom: { style: "single", size: 6, color: "BFBFBF" } },
          children: [],
          spacing: { after: 120 },
        })
      );
      continue;
    }
    if (tag === "script" || tag === "style") continue;
    // oxlint-disable-next-line no-await-in-loop -- blocks must keep DOM order
    blocks.push(...(await walkBlocks(Array.from(element.childNodes), ctx)));
  }
  return blocks;
}

export async function htmlToDocxBlocks(html: string, options: THtmlToDocxOptions = {}): Promise<TDocumentBlock[]> {
  const imageFetcher = options.imageFetcher ?? defaultImageFetcher;
  const maxImageWidth = options.maxImageWidth ?? MAX_IMAGE_WIDTH;
  const imageUnavailableText = options.imageUnavailableText ?? DEFAULT_IMAGE_UNAVAILABLE_TEXT;
  if (typeof DOMParser === "undefined") return [];
  const document = new DOMParser().parseFromString(html || "", "text/html");
  return walkBlocks(Array.from(document.body.childNodes), { imageFetcher, maxImageWidth, imageUnavailableText });
}

export const DOCX_NUMBERING_CONFIG = [
  {
    reference: DOCX_BULLET_REFERENCE,
    levels: [0, 1, 2].map((level) => ({
      level,
      format: LevelFormat.BULLET,
      text: "•",
      alignment: AlignmentType.LEFT,
      style: { paragraph: { indent: { left: 720 * (level + 1), hanging: 360 } } },
    })),
  },
  {
    reference: DOCX_NUMBERED_REFERENCE,
    levels: [0, 1, 2].map((level) => ({
      level,
      format: LevelFormat.DECIMAL,
      text: `%${level + 1}.`,
      alignment: AlignmentType.LEFT,
      style: { paragraph: { indent: { left: 720 * (level + 1), hanging: 360 } } },
    })),
  },
];
