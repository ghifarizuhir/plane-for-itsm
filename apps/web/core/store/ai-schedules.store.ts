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
  error: string | null;
  setWorkspace: (workspaceSlug: string) => void;
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
  error: string | null = null;

  private workspaceSlug: string | undefined = undefined;
  private requestSeq = 0;

  constructor(private service: TAiSchedulesService = new AiSchedulesService()) {
    makeObservable(this, {
      schedules: observable.deep,
      runsBySchedule: observable.deep,
      loader: observable.ref,
      error: observable.ref,
      setWorkspace: action,
      fetchSchedules: action,
      fetchRuns: action,
      toggleSchedule: action,
      deleteSchedule: action,
      runNow: action,
    });
  }

  setWorkspace = (workspaceSlug: string) => {
    if (workspaceSlug !== this.workspaceSlug) {
      this.requestSeq += 1;
      this.workspaceSlug = workspaceSlug;
      this.schedules = [];
      this.runsBySchedule = {};
      this.error = null;
      this.loader = false;
    }
  };

  fetchSchedules = async (workspaceSlug: string) => {
    const seq = ++this.requestSeq;
    runInAction(() => {
      this.loader = true;
      this.error = null;
    });
    try {
      const schedules = await this.service.list(workspaceSlug);
      if (seq !== this.requestSeq || workspaceSlug !== this.workspaceSlug) return;
      runInAction(() => {
        this.schedules = schedules ?? [];
        this.error = null;
      });
    } catch (err) {
      if (seq === this.requestSeq) {
        runInAction(() => {
          this.error = "Could not load schedules.";
        });
      }
      throw err;
    } finally {
      if (seq === this.requestSeq) {
        runInAction(() => {
          this.loader = false;
        });
      }
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
    try {
      await this.fetchSchedules(workspaceSlug);
    } catch {
      // keep the optimistic local state; the next poll will retry
    }
  };

  deleteSchedule = async (workspaceSlug: string, scheduleId: string) => {
    await this.service.remove(workspaceSlug, scheduleId);
    runInAction(() => {
      this.schedules = this.schedules.filter((schedule) => schedule.id !== scheduleId);
      delete this.runsBySchedule[scheduleId];
      this.requestSeq += 1;
      this.loader = false;
    });
  };

  runNow = async (workspaceSlug: string, scheduleId: string) => {
    await this.service.runNow(workspaceSlug, scheduleId);
    await Promise.allSettled([this.fetchRuns(workspaceSlug, scheduleId), this.fetchSchedules(workspaceSlug)]);
  };
}
