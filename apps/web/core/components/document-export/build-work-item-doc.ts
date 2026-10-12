/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { HeadingLevel, Paragraph, ShadingType, Table, TableCell, TableRow, TextRun, WidthType } from "docx";
import type { TDocumentExportTextLine } from "@plane/types";
import {
  DOCX_NUMBERING_CONFIG,
  htmlToDocxBlocks,
  type TDocxImage,
  type TDocumentBlock,
  type TImageFetcher,
} from "./html-to-docx";
import { packDocument } from "./shell";
import type { TMilestoneRow } from "./milestones";

export type TWorkItemLinkedItem = {
  identifier: string;
  title: string;
  relationLabel: string;
  stateName?: string;
};

export type TWorkItemDocLabels = {
  properties: string;
  description: string;
  linked: string;
  history: string;
  generatedBy: string;
  noValue: string;
  columnWhen: string;
  columnActor: string;
  columnEvent: string;
  columnChange: string;
};

export type TWorkItemDocInput = {
  title: string;
  projectName: string;
  stateName?: string;
  typeName?: string;
  priority?: string | null;
  assigneeNames: string[];
  labelNames: string[];
  startDate?: string | null;
  targetDate?: string | null;
  createdBy?: string;
  createdAt?: string;
  updatedBy?: string;
  updatedAt?: string;
  descriptionHtml: string;
  linkedItems: TWorkItemLinkedItem[];
  milestones: TMilestoneRow[];
  includeHistory: boolean;
  headerLines: TDocumentExportTextLine[];
  footerLines: TDocumentExportTextLine[];
  showPageNumbers: boolean;
  generatedByLine: string;
  creator: string;
  logo: TDocxImage | null;
  labels: TWorkItemDocLabels;
  workspaceName: string;
  imageFetcher?: TImageFetcher;
};

const BORDER = { style: "single" as const, size: 4, color: "D9D9D9" };

const sectionHeading = (text: string) =>
  new Paragraph({
    heading: HeadingLevel.HEADING_2,
    children: [new TextRun({ text })],
    spacing: { before: 240, after: 120 },
  });

const formatDate = (value?: string | null) => {
  if (!value) return undefined;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toISOString().slice(0, 10);
};

const gridTable = (rows: [string, string][]): Table =>
  new Table({
    width: { size: 100, type: WidthType.PERCENTAGE },
    borders: {
      top: BORDER,
      bottom: BORDER,
      left: BORDER,
      right: BORDER,
      insideHorizontal: BORDER,
      insideVertical: BORDER,
    },
    rows: rows.map(
      ([key, value]) =>
        new TableRow({
          children: [
            new TableCell({
              width: { size: 28, type: WidthType.PERCENTAGE },
              shading: { type: ShadingType.CLEAR, fill: "F7F7F7", color: "auto" },
              children: [new Paragraph({ children: [new TextRun({ text: key, bold: true, size: 20 })] })],
            }),
            new TableCell({
              children: [new Paragraph({ children: [new TextRun({ text: value, size: 20 })] })],
            }),
          ],
        })
    ),
  });

const dataTable = (headerCells: string[], rows: string[][]): Table =>
  new Table({
    width: { size: 100, type: WidthType.PERCENTAGE },
    borders: {
      top: BORDER,
      bottom: BORDER,
      left: BORDER,
      right: BORDER,
      insideHorizontal: BORDER,
      insideVertical: BORDER,
    },
    rows: [
      new TableRow({
        children: headerCells.map(
          (cell) =>
            new TableCell({
              shading: { type: ShadingType.CLEAR, fill: "F2F2F2", color: "auto" },
              children: [new Paragraph({ children: [new TextRun({ text: cell, bold: true, size: 20 })] })],
            })
        ),
      }),
      ...rows.map(
        (row) =>
          new TableRow({
            children: row.map(
              (cell) =>
                new TableCell({ children: [new Paragraph({ children: [new TextRun({ text: cell, size: 20 })] })] })
            ),
          })
      ),
    ],
  });

export async function buildWorkItemDocument(input: TWorkItemDocInput): Promise<Blob> {
  const children: TDocumentBlock[] = [
    new Paragraph({ heading: HeadingLevel.HEADING_1, children: [new TextRun({ text: input.title })] }),
  ];
  if (input.typeName) {
    children.push(new Paragraph({ children: [new TextRun({ text: input.typeName, size: 20, color: "595959" })] }));
  }

  children.push(sectionHeading(input.labels.properties));
  const propertyRows: [string, string][] = [];
  const push = (key: string, value?: string | null) => {
    if (value && value.trim()) propertyRows.push([key, value]);
  };
  push("State", input.stateName);
  push("Priority", input.priority);
  push("Assignees", input.assigneeNames.join(", "));
  push("Labels", input.labelNames.join(", "));
  push("Start date", formatDate(input.startDate));
  push("Target date", formatDate(input.targetDate));
  push("Project", input.projectName);
  push("Created", [input.createdBy, formatDate(input.createdAt)].filter(Boolean).join(" · "));
  push("Last updated", [input.updatedBy, formatDate(input.updatedAt)].filter(Boolean).join(" · "));
  children.push(gridTable(propertyRows));

  if (input.descriptionHtml?.trim()) {
    children.push(sectionHeading(input.labels.description));
    children.push(...(await htmlToDocxBlocks(input.descriptionHtml, { imageFetcher: input.imageFetcher })));
  }

  if (input.linkedItems.length) {
    children.push(sectionHeading(input.labels.linked));
    children.push(
      dataTable(
        ["ID", "Title", "Relation", "State"],
        input.linkedItems.map((item) => [
          item.identifier,
          item.title,
          item.relationLabel,
          item.stateName ?? input.labels.noValue,
        ])
      )
    );
  }

  if (input.includeHistory) {
    children.push(sectionHeading(input.labels.history));
    if (input.milestones.length) {
      children.push(
        dataTable(
          [input.labels.columnWhen, input.labels.columnActor, input.labels.columnEvent, input.labels.columnChange],
          input.milestones.map((row) => [row.createdAt, row.actor, row.event, `${row.from} → ${row.to}`])
        )
      );
    } else {
      children.push(new Paragraph({ children: [new TextRun({ text: input.labels.noValue, size: 20 })] }));
    }
  }

  return packDocument({
    title: input.title,
    creator: input.creator,
    children,
    headerLines: input.headerLines,
    footerLines: input.footerLines,
    showPageNumbers: input.showPageNumbers,
    generatedByLine: input.generatedByLine,
    logo: input.logo,
    numbering: DOCX_NUMBERING_CONFIG,
  });
}
