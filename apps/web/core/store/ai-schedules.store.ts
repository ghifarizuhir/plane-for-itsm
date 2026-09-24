/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { action, makeObservable, observable, runInAction } from "mobx";
import { AiSchedulesService } from "@/services/ai-schedules.service";
import type { TAiSchedule, TAiScheduleRun } from "@/lib/ai-schedule";

type TAiSchedulesService = Pick<AiSchedulesService, "list" | "retrieve" | "update" | "remove" | "runNow">;

export interface IAiSchedulesStore {
  schedules: TAiSchedule[];
  runsBySchedule: Record<string, TAiScheduleRun[]>;
  loader: boolean;
  fetchSchedules: (workspaceSlug: string) => Promise<void>;
  fetchRuns: (workspaceSlug: string, scheduleId: string) => Promise<void>;
  toggleSchedule: (workspaceSlug: string, scheduleId: string, enabled: boolean) => Promise<void>;
  deleteSchedule: (workspaceSlug: string, scheduleId: string) => Promise<void>;
  runNow: (workspaceSlug: string, scheduleId: string) => Promise<void>;
}

export class AiSchedulesStore implements IAiSchedulesStore {
  schedules: TAiSchedule[] = [];
  runsBySchedule: Record<string, TAiScheduleRun[]> = {};
  loader = false;

  constructor(private service: TAiSchedulesService = new AiSchedulesService()) {
    makeObservable(this, {
      schedules: observable.deep,
      runsBySchedule: observable.deep,
      loader: observable.ref,
      fetchSchedules: action,
      fetchRuns: action,
      toggleSchedule: action,
      deleteSchedule: action,
      runNow: action,
    });
  }

  fetchSchedules = async (workspaceSlug: string) => {
    this.loader = true;
    try {
      const schedules = await this.service.list(workspaceSlug);
      runInAction(() => {
        this.schedules = schedules ?? [];
      });
    } finally {
      runInAction(() => {
        this.loader = false;
      });
    }
  };

  fetchRuns = async (workspaceSlug: string, scheduleId: string) => {
    const schedule = await this.service.retrieve(workspaceSlug, scheduleId);
    runInAction(() => {
      this.runsBySchedule[scheduleId] = schedule?.runs ?? [];
    });
  };

  toggleSchedule = async (workspaceSlug: string, scheduleId: string, enabled: boolean) => {
    await this.service.update(workspaceSlug, scheduleId, enabled);
    runInAction(() => {
      this.schedules = this.schedules.map((schedule) =>
        schedule.id === scheduleId ? { ...schedule, enabled } : schedule
      );
    });
  };

  deleteSchedule = async (workspaceSlug: string, scheduleId: string) => {
    await this.service.remove(workspaceSlug, scheduleId);
    runInAction(() => {
      this.schedules = this.schedules.filter((schedule) => schedule.id !== scheduleId);
    });
  };

  runNow = async (workspaceSlug: string, scheduleId: string) => {
    await this.service.runNow(workspaceSlug, scheduleId);
    await this.fetchRuns(workspaceSlug, scheduleId);
  };
}
