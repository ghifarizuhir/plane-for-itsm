/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Extension, onConfigurePayload } from "@hocuspocus/server";
import { logger } from "@plane/logger";
import { Redis } from "@/extensions/redis";
import { replaceDocumentContent } from "@/lib/replace-document";
import { AdminCommand, isApplyDocumentCommand } from "@/types/admin-commands";
import type { ApplyDocumentCommandData } from "@/types/admin-commands";

/**
 * Applies external document updates (e.g. AI article edits) to live
 * collaborative documents. If the document is not in memory there is nothing
 * to update: the REST write already persisted the content and the next load
 * will fetch it.
 */
export class ApplyDocumentHandler implements Extension {
  name = "ApplyDocumentHandler";
  priority = 998;

  async onConfigure({ instance }: onConfigurePayload) {
    const redisExt = instance.configuration.extensions.find((ext) => ext instanceof Redis);

    if (!redisExt) {
      logger.warn("[APPLY_DOCUMENT_HANDLER] Redis extension not found");
      return;
    }

    redisExt.onAdminCommand<ApplyDocumentCommandData>(AdminCommand.APPLY_DOCUMENT, (data) => {
      if (!isApplyDocumentCommand(data)) {
        logger.error("[APPLY_DOCUMENT_HANDLER] Received invalid apply_document command");
        return;
      }

      const document = instance.documents.get(data.docId);
      if (!document) {
        logger.info(`[APPLY_DOCUMENT_HANDLER] No live document for ${data.docId}, skipping`);
        return;
      }

      try {
        replaceDocumentContent(document, data.descriptionHtml, data.name);
        logger.info(`[APPLY_DOCUMENT_HANDLER] Applied external update to ${data.docId}`);
      } catch (error) {
        logger.error("[APPLY_DOCUMENT_HANDLER] Failed to apply document update", error);
      }
    });

    logger.info("[APPLY_DOCUMENT_HANDLER] Registered with Redis extension");
  }
}
