/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { set } from "lodash-es";
import { action, observable, makeObservable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type {
  IWarRoom,
  IWarRoomEvent,
  IWarRoomListItem,
  IWarRoomMessage,
  IWarRoomParticipant,
  IWarRoomRunbookItem,
  IWarRoomSummary,
  TWarRoomCreatePayload,
  TWarRoomEventsParams,
  TWarRoomListParams,
  TWarRoomMessagesParams,
  TWarRoomParticipantCreatePayload,
  TWarRoomParticipantUpdatePayload,
  TWarRoomRunbookUpdatePayload,
  TWarRoomSocketEvent,
  TWarRoomUpdatePayload,
} from "@plane/types";
// helpers
import { isActiveWarRoomStatus, isTypingActive, upsertMessageInList } from "@/services/war-room.helpers";
// services
import { WarRoomService } from "@/services/war-room.service";
// store
import type { CoreRootStore } from "./root.store";

const TYPING_TIMEOUT_MS = 3000;
const DEFAULT_MESSAGE_PAGE_SIZE = 50;
const DEFAULT_EVENT_PAGE_SIZE = 50;

export interface IWarRoomStore {
  loader: boolean;
  fetchedMap: Record<string, boolean>;
  warRoomMap: Record<string, IWarRoomListItem>;
  warRoomIdsMap: Record<string, string[]>;
  detailMap: Record<string, IWarRoom>;
  summaryMap: Record<string, IWarRoomSummary>;
  errorMap: Record<string, boolean>;
  detailErrorMap: Record<string, boolean>;
  messagesMap: Record<string, IWarRoomMessage[]>;
  messagesHasMoreMap: Record<string, boolean>;
  messagesLoaderMap: Record<string, boolean>;
  eventsMap: Record<string, IWarRoomEvent[]>;
  eventsHasMoreMap: Record<string, boolean>;
  eventsLoaderMap: Record<string, boolean>;
  unreadMap: Record<string, number>;
  typingMap: Record<string, Record<string, number>>;
  onlineUsersMap: Record<string, string[]>;
  getWarRoomById: (warRoomId: string) => IWarRoomListItem | null;
  getProjectWarRoomIds: (projectId: string) => string[] | null;
  getWarRoomDetailById: (warRoomId: string) => IWarRoom | null;
  getProjectSummary: (projectId: string) => IWarRoomSummary | null;
  getActiveWarRoomByIssue: (projectId: string, issueId: string) => IWarRoomListItem | null;
  getMessages: (warRoomId: string) => IWarRoomMessage[];
  hasMoreMessages: (warRoomId: string) => boolean;
  getEvents: (warRoomId: string) => IWarRoomEvent[];
  hasMoreEvents: (warRoomId: string) => boolean;
  getUnreadCount: (warRoomId: string) => number;
  getTypingUserIds: (warRoomId: string, now?: number) => string[];
  getOnlineUserIds: (warRoomId: string) => string[];
  fetchWarRooms: (
    workspaceSlug: string,
    projectId: string,
    params?: TWarRoomListParams
  ) => Promise<IWarRoomListItem[] | undefined>;
  fetchWarRoomSummary: (workspaceSlug: string, projectId: string) => Promise<IWarRoomSummary | undefined>;
  fetchWarRoomDetail: (workspaceSlug: string, projectId: string, warRoomId: string) => Promise<IWarRoom | undefined>;
  createWarRoom: (workspaceSlug: string, projectId: string, data: TWarRoomCreatePayload) => Promise<IWarRoom>;
  updateWarRoom: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomUpdatePayload
  ) => Promise<IWarRoom>;
  deleteWarRoom: (workspaceSlug: string, projectId: string, warRoomId: string) => Promise<void>;
  addServices: (workspaceSlug: string, projectId: string, warRoomId: string, serviceIds: string[]) => Promise<number>;
  removeService: (workspaceSlug: string, projectId: string, warRoomId: string, serviceId: string) => Promise<void>;
  addIssues: (workspaceSlug: string, projectId: string, warRoomId: string, issueIds: string[]) => Promise<number>;
  removeIssue: (workspaceSlug: string, projectId: string, warRoomId: string, issueId: string) => Promise<void>;
  addParticipant: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomParticipantCreatePayload
  ) => Promise<IWarRoomParticipant>;
  updateParticipantRole: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    participantId: string,
    data: TWarRoomParticipantUpdatePayload
  ) => Promise<IWarRoomParticipant>;
  removeParticipant: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    participantId: string
  ) => Promise<void>;
  createRunbookItem: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    title: string
  ) => Promise<IWarRoomRunbookItem>;
  updateRunbookItem: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    itemId: string,
    data: TWarRoomRunbookUpdatePayload
  ) => Promise<IWarRoomRunbookItem>;
  deleteRunbookItem: (workspaceSlug: string, projectId: string, warRoomId: string, itemId: string) => Promise<void>;
  fetchMessages: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    params?: TWarRoomMessagesParams
  ) => Promise<IWarRoomMessage[] | undefined>;
  sendMessage: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    body: string,
    clientId: string
  ) => Promise<IWarRoomMessage>;
  updateMessage: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    messageId: string,
    body: string
  ) => Promise<IWarRoomMessage>;
  deleteMessage: (workspaceSlug: string, projectId: string, warRoomId: string, messageId: string) => Promise<void>;
  fetchEvents: (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    params?: TWarRoomEventsParams
  ) => Promise<IWarRoomEvent[] | undefined>;
  applySocketEvent: (workspaceSlug: string, projectId: string, warRoomId: string, event: TWarRoomSocketEvent) => void;
  incrementUnread: (warRoomId: string) => void;
  clearUnread: (warRoomId: string) => void;
}

export class WarRoomStore implements IWarRoomStore {
  loader: boolean = false;
  fetchedMap: Record<string, boolean> = {};
  warRoomMap: Record<string, IWarRoomListItem> = {};
  warRoomIdsMap: Record<string, string[]> = {};
  detailMap: Record<string, IWarRoom> = {};
  summaryMap: Record<string, IWarRoomSummary> = {};
  errorMap: Record<string, boolean> = {};
  detailErrorMap: Record<string, boolean> = {};
  messagesMap: Record<string, IWarRoomMessage[]> = {};
  messagesHasMoreMap: Record<string, boolean> = {};
  messagesLoaderMap: Record<string, boolean> = {};
  eventsMap: Record<string, IWarRoomEvent[]> = {};
  eventsHasMoreMap: Record<string, boolean> = {};
  eventsLoaderMap: Record<string, boolean> = {};
  unreadMap: Record<string, number> = {};
  typingMap: Record<string, Record<string, number>> = {};
  onlineUsersMap: Record<string, string[]> = {};
  rootStore: CoreRootStore;
  warRoomService: WarRoomService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      fetchedMap: observable,
      warRoomMap: observable,
      warRoomIdsMap: observable,
      detailMap: observable,
      summaryMap: observable,
      errorMap: observable,
      detailErrorMap: observable,
      messagesMap: observable,
      messagesHasMoreMap: observable,
      messagesLoaderMap: observable,
      eventsMap: observable,
      eventsHasMoreMap: observable,
      eventsLoaderMap: observable,
      unreadMap: observable,
      typingMap: observable,
      onlineUsersMap: observable,
      fetchWarRooms: action,
      fetchWarRoomSummary: action,
      fetchWarRoomDetail: action,
      createWarRoom: action,
      updateWarRoom: action,
      deleteWarRoom: action,
      addServices: action,
      removeService: action,
      addIssues: action,
      removeIssue: action,
      addParticipant: action,
      updateParticipantRole: action,
      removeParticipant: action,
      createRunbookItem: action,
      updateRunbookItem: action,
      deleteRunbookItem: action,
      fetchMessages: action,
      sendMessage: action,
      updateMessage: action,
      deleteMessage: action,
      fetchEvents: action,
      applySocketEvent: action,
      applyTyping: action,
      applyPresence: action,
      incrementUnread: action,
      clearUnread: action,
    });
    this.rootStore = _rootStore;
    this.warRoomService = new WarRoomService();
  }

  getWarRoomById = computedFn((warRoomId: string) => this.warRoomMap[warRoomId] ?? null);

  getProjectWarRoomIds = computedFn((projectId: string) =>
    this.fetchedMap[projectId] ? (this.warRoomIdsMap[projectId] ?? []) : null
  );

  getWarRoomDetailById = computedFn((warRoomId: string) => this.detailMap[warRoomId] ?? null);

  getProjectSummary = computedFn((projectId: string) => this.summaryMap[projectId] ?? null);

  getActiveWarRoomByIssue = computedFn((projectId: string, issueId: string) => {
    const room = Object.values(this.warRoomMap).find(
      (candidate) =>
        candidate.project_id === projectId &&
        candidate.primary_issue_id === issueId &&
        isActiveWarRoomStatus(candidate.status)
    );
    return room ?? null;
  });

  getMessages = (warRoomId: string): IWarRoomMessage[] => this.messagesMap[warRoomId] ?? [];

  hasMoreMessages = (warRoomId: string): boolean => this.messagesHasMoreMap[warRoomId] ?? false;

  getEvents = (warRoomId: string): IWarRoomEvent[] => this.eventsMap[warRoomId] ?? [];

  hasMoreEvents = (warRoomId: string): boolean => this.eventsHasMoreMap[warRoomId] ?? false;

  getUnreadCount = (warRoomId: string): number => this.unreadMap[warRoomId] ?? 0;

  getTypingUserIds = (warRoomId: string, now: number = Date.now()): string[] =>
    Object.entries(this.typingMap[warRoomId] ?? {})
      .filter(([, expiresAt]) => isTypingActive(expiresAt, now))
      .map(([userId]) => userId);

  getOnlineUserIds = (warRoomId: string): string[] => this.onlineUsersMap[warRoomId] ?? [];

  fetchWarRooms = async (workspaceSlug: string, projectId: string, params?: TWarRoomListParams) => {
    try {
      runInAction(() => {
        set(this.errorMap, projectId, false);
        this.loader = true;
      });
      const rooms = await this.warRoomService.getWarRooms(workspaceSlug, projectId, params);
      runInAction(() => {
        rooms.forEach((room) => set(this.warRoomMap, [room.id], room));
        set(
          this.warRoomIdsMap,
          projectId,
          rooms.map((room) => room.id)
        );
        set(this.fetchedMap, projectId, true);
        this.loader = false;
      });
      return rooms;
    } catch {
      runInAction(() => {
        this.loader = false;
        set(this.errorMap, projectId, true);
      });
      return undefined;
    }
  };

  fetchWarRoomSummary = async (workspaceSlug: string, projectId: string) => {
    try {
      const summary = await this.warRoomService.getWarRoomSummary(workspaceSlug, projectId);
      runInAction(() => {
        set(this.summaryMap, projectId, summary);
      });
      return summary;
    } catch {
      return undefined;
    }
  };

  fetchWarRoomDetail = async (workspaceSlug: string, projectId: string, warRoomId: string) => {
    try {
      runInAction(() => {
        set(this.detailErrorMap, warRoomId, false);
      });
      const room = await this.warRoomService.getWarRoom(workspaceSlug, projectId, warRoomId);
      runInAction(() => {
        set(this.detailMap, [warRoomId], room);
      });
      return room;
    } catch {
      runInAction(() => {
        set(this.detailErrorMap, warRoomId, true);
      });
      return undefined;
    }
  };

  createWarRoom = async (workspaceSlug: string, projectId: string, data: TWarRoomCreatePayload) => {
    const room = await this.warRoomService.createWarRoom(workspaceSlug, projectId, data);
    runInAction(() => {
      set(this.detailMap, [room.id], room);
      const currentIds = this.warRoomIdsMap[projectId] ?? [];
      set(this.warRoomIdsMap, projectId, [room.id, ...currentIds.filter((id) => id !== room.id)]);
      set(this.fetchedMap, projectId, true);
    });
    return room;
  };

  updateWarRoom = async (workspaceSlug: string, projectId: string, warRoomId: string, data: TWarRoomUpdatePayload) => {
    const room = await this.warRoomService.updateWarRoom(workspaceSlug, projectId, warRoomId, data);
    runInAction(() => {
      set(this.detailMap, [warRoomId], room);
      const listItem = this.warRoomMap[warRoomId];
      if (listItem) {
        set(this.warRoomMap, [warRoomId], {
          ...listItem,
          name: room.name,
          description_html: room.description_html,
          notes_html: room.notes_html,
          severity: room.severity,
          status: room.status,
          resolved_at: room.resolved_at,
          updated_at: room.updated_at,
        });
      }
    });
    return room;
  };

  deleteWarRoom = async (workspaceSlug: string, projectId: string, warRoomId: string) => {
    await this.warRoomService.deleteWarRoom(workspaceSlug, projectId, warRoomId);
    runInAction(() => {
      delete this.detailMap[warRoomId];
      delete this.warRoomMap[warRoomId];
      delete this.messagesMap[warRoomId];
      delete this.messagesHasMoreMap[warRoomId];
      delete this.eventsMap[warRoomId];
      delete this.eventsHasMoreMap[warRoomId];
      const currentIds = this.warRoomIdsMap[projectId];
      if (currentIds)
        set(
          this.warRoomIdsMap,
          projectId,
          currentIds.filter((id) => id !== warRoomId)
        );
    });
  };

  addServices = async (workspaceSlug: string, projectId: string, warRoomId: string, serviceIds: string[]) => {
    const response = await this.warRoomService.addServices(workspaceSlug, projectId, warRoomId, {
      service_ids: serviceIds,
    });
    await this.fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
    return response.linked;
  };

  removeService = async (workspaceSlug: string, projectId: string, warRoomId: string, serviceId: string) => {
    await this.warRoomService.removeService(workspaceSlug, projectId, warRoomId, serviceId);
    await this.fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
  };

  addIssues = async (workspaceSlug: string, projectId: string, warRoomId: string, issueIds: string[]) => {
    const response = await this.warRoomService.addIssues(workspaceSlug, projectId, warRoomId, {
      issue_ids: issueIds,
    });
    await this.fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
    return response.linked;
  };

  removeIssue = async (workspaceSlug: string, projectId: string, warRoomId: string, issueId: string) => {
    await this.warRoomService.removeIssue(workspaceSlug, projectId, warRoomId, issueId);
    await this.fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
  };

  addParticipant = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomParticipantCreatePayload
  ) => {
    const participant = await this.warRoomService.createParticipant(workspaceSlug, projectId, warRoomId, data);
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      const existing = room.participants.some((candidate) => candidate.id === participant.id);
      const participants = existing
        ? room.participants.map((candidate) => (candidate.id === participant.id ? participant : candidate))
        : [...room.participants, participant];
      set(this.detailMap, [warRoomId], {
        ...room,
        participants:
          participant.role === "commander"
            ? // oxlint-disable-next-line oxc(no-map-spread) -- copy-on-write is required for MobX observables
              participants.map((candidate) =>
                candidate.id !== participant.id && candidate.role === "commander"
                  ? { ...candidate, role: "responder" as const }
                  : candidate
              )
            : participants,
      });
    });
    return participant;
  };

  updateParticipantRole = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    participantId: string,
    data: TWarRoomParticipantUpdatePayload
  ) => {
    const participant = await this.warRoomService.updateParticipant(
      workspaceSlug,
      projectId,
      warRoomId,
      participantId,
      data
    );
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      set(this.detailMap, [warRoomId], {
        ...room,
        // oxlint-disable-next-line oxc(no-map-spread) -- copy-on-write is required for MobX observables
        participants: room.participants.map((candidate) => {
          if (candidate.id === participant.id) return participant;
          if (data.role === "commander" && candidate.role === "commander")
            return { ...candidate, role: "responder" as const };
          return candidate;
        }),
      });
    });
    return participant;
  };

  removeParticipant = async (workspaceSlug: string, projectId: string, warRoomId: string, participantId: string) => {
    await this.warRoomService.deleteParticipant(workspaceSlug, projectId, warRoomId, participantId);
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      set(this.detailMap, [warRoomId], {
        ...room,
        participants: room.participants.filter((candidate) => candidate.id !== participantId),
      });
    });
  };

  createRunbookItem = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    title: string
  ): Promise<IWarRoomRunbookItem> => {
    const item = await this.warRoomService.createRunbookItem(workspaceSlug, projectId, warRoomId, { title });
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      set(this.detailMap, [warRoomId], { ...room, runbook_items: [...room.runbook_items, item] });
    });
    return item;
  };

  updateRunbookItem = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    itemId: string,
    data: TWarRoomRunbookUpdatePayload
  ): Promise<IWarRoomRunbookItem> => {
    const item = await this.warRoomService.updateRunbookItem(workspaceSlug, projectId, warRoomId, itemId, data);
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      set(this.detailMap, [warRoomId], {
        ...room,
        runbook_items: room.runbook_items.map((candidate) => (candidate.id === item.id ? item : candidate)),
      });
    });
    return item;
  };

  deleteRunbookItem = async (workspaceSlug: string, projectId: string, warRoomId: string, itemId: string) => {
    await this.warRoomService.deleteRunbookItem(workspaceSlug, projectId, warRoomId, itemId);
    runInAction(() => {
      const room = this.detailMap[warRoomId];
      if (!room) return;
      set(this.detailMap, [warRoomId], {
        ...room,
        runbook_items: room.runbook_items.filter((candidate) => candidate.id !== itemId),
      });
    });
  };

  fetchMessages = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    params?: TWarRoomMessagesParams
  ) => {
    const isPaging = Boolean(params?.before_id);
    runInAction(() => {
      set(this.messagesLoaderMap, warRoomId, true);
    });
    try {
      const page = await this.warRoomService.getMessages(workspaceSlug, projectId, warRoomId, params);
      runInAction(() => {
        const existing = this.messagesMap[warRoomId] ?? [];
        if (isPaging) {
          const existingIds = new Set(existing.map((message) => message.id));
          set(this.messagesMap, warRoomId, [...page.filter((message) => !existingIds.has(message.id)), ...existing]);
        } else {
          set(this.messagesMap, warRoomId, page);
        }
        set(this.messagesHasMoreMap, warRoomId, page.length >= (params?.limit ?? DEFAULT_MESSAGE_PAGE_SIZE));
        set(this.messagesLoaderMap, warRoomId, false);
      });
      return page;
    } catch (error) {
      runInAction(() => {
        set(this.messagesLoaderMap, warRoomId, false);
      });
      throw error;
    }
  };

  sendMessage = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    body: string,
    clientId: string
  ): Promise<IWarRoomMessage> => {
    const user = this.rootStore.user?.data;
    const optimistic: IWarRoomMessage = {
      id: `optimistic-${clientId}`,
      war_room_id: warRoomId,
      author_id: user?.id ?? null,
      author: user
        ? { id: user.id, display_name: user.display_name ?? null, avatar_url: user.avatar_url ?? null }
        : null,
      body,
      mentions: [],
      edited_at: null,
      created_at: new Date().toISOString(),
      client_id: clientId,
    };
    runInAction(() => {
      set(this.messagesMap, warRoomId, [...(this.messagesMap[warRoomId] ?? []), optimistic]);
    });
    try {
      const message = await this.warRoomService.createMessage(workspaceSlug, projectId, warRoomId, {
        body,
        client_id: clientId,
      });
      runInAction(() => {
        set(this.messagesMap, warRoomId, upsertMessageInList(this.messagesMap[warRoomId] ?? [], message));
      });
      return message;
    } catch (error) {
      runInAction(() => {
        set(
          this.messagesMap,
          warRoomId,
          (this.messagesMap[warRoomId] ?? []).filter((message) => message.id !== optimistic.id)
        );
      });
      throw error;
    }
  };

  updateMessage = async (
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    messageId: string,
    body: string
  ): Promise<IWarRoomMessage> => {
    const message = await this.warRoomService.updateMessage(workspaceSlug, projectId, warRoomId, messageId, { body });
    runInAction(() => {
      set(this.messagesMap, warRoomId, upsertMessageInList(this.messagesMap[warRoomId] ?? [], message));
    });
    return message;
  };

  deleteMessage = async (workspaceSlug: string, projectId: string, warRoomId: string, messageId: string) => {
    await this.warRoomService.deleteMessage(workspaceSlug, projectId, warRoomId, messageId);
    runInAction(() => {
      set(
        this.messagesMap,
        warRoomId,
        (this.messagesMap[warRoomId] ?? []).filter((message) => message.id !== messageId)
      );
    });
  };

  fetchEvents = async (workspaceSlug: string, projectId: string, warRoomId: string, params?: TWarRoomEventsParams) => {
    const isPaging = Boolean(params?.before_id);
    runInAction(() => {
      set(this.eventsLoaderMap, warRoomId, true);
    });
    try {
      const page = await this.warRoomService.getEvents(workspaceSlug, projectId, warRoomId, params);
      runInAction(() => {
        const existing = this.eventsMap[warRoomId] ?? [];
        if (isPaging) {
          const existingIds = new Set(existing.map((event) => event.id));
          set(this.eventsMap, warRoomId, [...existing, ...page.filter((event) => !existingIds.has(event.id))]);
        } else {
          set(this.eventsMap, warRoomId, page);
        }
        set(this.eventsHasMoreMap, warRoomId, page.length >= (params?.limit ?? DEFAULT_EVENT_PAGE_SIZE));
        set(this.eventsLoaderMap, warRoomId, false);
      });
      return page;
    } catch (error) {
      runInAction(() => {
        set(this.eventsLoaderMap, warRoomId, false);
      });
      throw error;
    }
  };

  applySocketEvent = (workspaceSlug: string, projectId: string, warRoomId: string, event: TWarRoomSocketEvent) => {
    switch (event.kind) {
      case "message.created":
      case "message.updated":
        runInAction(() => {
          set(this.messagesMap, warRoomId, upsertMessageInList(this.messagesMap[warRoomId] ?? [], event.data));
        });
        break;
      case "message.deleted":
        runInAction(() => {
          set(
            this.messagesMap,
            warRoomId,
            (this.messagesMap[warRoomId] ?? []).filter((message) => message.id !== event.data.id)
          );
        });
        break;
      case "activity.created":
        runInAction(() => {
          const existing = this.eventsMap[warRoomId] ?? [];
          if (!existing.some((candidate) => candidate.id === event.data.id)) {
            set(this.eventsMap, warRoomId, [event.data, ...existing]);
          }
        });
        break;
      case "room.changed":
        if (event.data.reasons.includes("deleted")) {
          runInAction(() => {
            delete this.detailMap[warRoomId];
            delete this.warRoomMap[warRoomId];
            delete this.messagesMap[warRoomId];
            delete this.eventsMap[warRoomId];
          });
        } else {
          void this.fetchWarRoomDetail(workspaceSlug, projectId, warRoomId);
        }
        break;
      case "typing":
        this.applyTyping(warRoomId, event.data.user_id, event.data.is_typing);
        break;
      case "presence.joined":
        this.applyPresence(warRoomId, event.data.user_id, true);
        break;
      case "presence.left":
        this.applyPresence(warRoomId, event.data.user_id, false);
        break;
      default:
        break;
    }
  };

  applyTyping = (warRoomId: string, userId: string, isTyping: boolean) => {
    runInAction(() => {
      set(this.typingMap, warRoomId, {
        ...this.typingMap[warRoomId],
        [userId]: isTyping ? Date.now() + TYPING_TIMEOUT_MS : 0,
      });
    });
  };

  applyPresence = (warRoomId: string, userId: string, isOnline: boolean) => {
    runInAction(() => {
      const current = this.onlineUsersMap[warRoomId] ?? [];
      set(
        this.onlineUsersMap,
        warRoomId,
        isOnline ? [...new Set([...current, userId])] : current.filter((id) => id !== userId)
      );
    });
  };

  incrementUnread = (warRoomId: string) => {
    runInAction(() => {
      set(this.unreadMap, warRoomId, (this.unreadMap[warRoomId] ?? 0) + 1);
    });
  };

  clearUnread = (warRoomId: string) => {
    runInAction(() => {
      set(this.unreadMap, warRoomId, 0);
    });
  };
}
