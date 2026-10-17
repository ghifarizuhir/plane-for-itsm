/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { lazy } from "react";
import type { ComponentType, ElementType, LazyExoticComponent } from "react";
import {
  AiStar1Outline,
  AlertOctagonOutline,
  AnalyticsOutline,
  CalendarOutline,
  ConnectionsOutline,
  CyclesOutline,
  DraftsOutline,
  FilterOutline,
  HomeOutline,
  InboxOutline,
  IntakeOutline,
  ModuleOutline,
  MultipleStickyOutline,
  OverviewOutline,
  PagesOutline,
  PreferencesOutline,
  SearchOutline,
  ServerOutline,
  SettingsOutline,
  ViewsOutline,
  WorkItemsOutline,
  YourWorkOutline,
} from "@makeplane/propel/icons";

export type TDocGroupKey = "getting-started" | "member" | "project" | "itil4" | "settings";

export const DOC_TOPIC_GROUPS: { key: TDocGroupKey; label: string }[] = [
  { key: "getting-started", label: "Memulai" },
  { key: "member", label: "Untuk Member" },
  { key: "project", label: "Untuk Project" },
  { key: "itil4", label: "ITIL4" },
  { key: "settings", label: "Pengaturan" },
];

export type TDocTopic = {
  slug: string;
  title: string;
  description: string;
  group: TDocGroupKey;
  icon: ElementType;
  component: LazyExoticComponent<ComponentType>;
};

export const DEFAULT_DOC_TOPIC_SLUG = "overview";

export const DOC_TOPICS: TDocTopic[] = [
  {
    slug: "overview",
    title: "Apa itu platform ini?",
    description: "Kenalan dulu: apa yang bisa dilakukan platform ini dan bagaimana istilah-istilahnya dipetakan.",
    group: "getting-started",
    icon: OverviewOutline,
    component: lazy(() => import("./content/overview")),
  },
  {
    slug: "home",
    title: "Home",
    description: "Dasbor awal: ringkasan pekerjaanmu dan aktivitas terbaru di workspace.",
    group: "member",
    icon: HomeOutline,
    component: lazy(() => import("./content/home")),
  },
  {
    slug: "inbox",
    title: "Inbox",
    description: "Semua pemberitahuan penting di satu tempat: mention, assignment, komentar, dan perubahan status.",
    group: "member",
    icon: InboxOutline,
    component: lazy(() => import("./content/inbox")),
  },
  {
    slug: "your-work",
    title: "Your work",
    description: "Halaman pribadi: semua pekerjaanmu lintas project dalam satu tampilan.",
    group: "member",
    icon: YourWorkOutline,
    component: lazy(() => import("./content/your-work")),
  },
  {
    slug: "reports",
    title: "Reports",
    description: "Analitik workspace: tren pekerjaan, distribusi status, dan gambaran beban tim.",
    group: "member",
    icon: AnalyticsOutline,
    component: lazy(() => import("./content/reports")),
  },
  {
    slug: "workspace-views",
    title: "Workspace Views",
    description: "Kumpulan ticket lintas project yang tersimpan: semua, milikmu, kamu buat, dan yang kamu ikuti.",
    group: "member",
    icon: ViewsOutline,
    component: lazy(() => import("./content/workspace-views")),
  },
  {
    slug: "search",
    title: "Pencarian & command palette",
    description: "Tekan Cmd/Ctrl + K untuk mencari apa saja dan menjalankan aksi tanpa mouse.",
    group: "member",
    icon: SearchOutline,
    component: lazy(() => import("./content/search")),
  },
  {
    slug: "drafts",
    title: "Drafts",
    description: "Tempat menyimpan ticket yang belum siap kamu buat — tulisan setengah jadi tetap tersimpan.",
    group: "member",
    icon: DraftsOutline,
    component: lazy(() => import("./content/drafts")),
  },
  {
    slug: "stickies",
    title: "Stickies",
    description: "Catatan tempel digital untuk hal-hal cepat yang tidak perlu menjadi ticket.",
    group: "member",
    icon: MultipleStickyOutline,
    component: lazy(() => import("./content/stickies")),
  },
  {
    slug: "scheduler",
    title: "Scheduler",
    description: "Jadwalkan tugas AI berulang: laporan rutin, rangkuman, atau pengingat otomatis.",
    group: "member",
    icon: CalendarOutline,
    component: lazy(() => import("./content/scheduler")),
  },
  {
    slug: "galileo",
    title: "Galileo",
    description: "Asisten AI di sisi kanan layar: tanya jawab dengan konteks pekerjaanmu.",
    group: "member",
    icon: AiStar1Outline,
    component: lazy(() => import("./content/galileo")),
  },
  {
    slug: "personal-settings",
    title: "Profil & preferensi",
    description: "Atur profilmu: nama, tema, bahasa, notifikasi, keamanan, dan API token.",
    group: "member",
    icon: PreferencesOutline,
    component: lazy(() => import("./content/personal-settings")),
  },
  {
    slug: "tickets",
    title: "Tickets",
    description: "Unit kerja utama: setiap insiden, permintaan, atau tugas direpresentasikan sebagai satu ticket.",
    group: "project",
    icon: WorkItemsOutline,
    component: lazy(() => import("./content/tickets")),
  },
  {
    slug: "sprints",
    title: "Sprints",
    description: "Periode kerja berulang untuk merencanakan dan menyelesaikan pekerjaan dalam rentang waktu tetap.",
    group: "project",
    icon: CyclesOutline,
    component: lazy(() => import("./content/sprints")),
  },
  {
    slug: "tracks",
    title: "Tracks",
    description: "Kelompok pekerjaan yang lebih besar: sekumpulan ticket yang menuju satu sasaran atau fase.",
    group: "project",
    icon: ModuleOutline,
    component: lazy(() => import("./content/tracks")),
  },
  {
    slug: "services",
    title: "Services",
    description: "Katalog layanan IT: apa yang kami sediakan, siapa pemiliknya, dan bagaimana kesehatannya.",
    group: "project",
    icon: ServerOutline,
    component: lazy(() => import("./content/services")),
  },
  {
    slug: "war-rooms",
    title: "War Rooms",
    description: "Ruang insiden untuk major incident: koordinasi cepat dalam satu tempat.",
    group: "project",
    icon: AlertOctagonOutline,
    component: lazy(() => import("./content/war-rooms")),
  },
  {
    slug: "queues",
    title: "Queues",
    description: "Antrean kerja tersaring: simpan filter favoritmu sebagai view yang bisa dibuka tim.",
    group: "project",
    icon: FilterOutline,
    component: lazy(() => import("./content/queues")),
  },
  {
    slug: "knowledge-base",
    title: "Knowledge base",
    description: "Dokumentasi tim: artikel, panduan, dan catatan solusi — semuanya bisa dicari.",
    group: "project",
    icon: PagesOutline,
    component: lazy(() => import("./content/knowledge-base")),
  },
  {
    slug: "intake",
    title: "Intake",
    description: "Pintu masuk permintaan: triage laporan baru sebelum menjadi ticket.",
    group: "project",
    icon: IntakeOutline,
    component: lazy(() => import("./content/intake")),
  },
  {
    slug: "itil4-overview",
    title: "ITIL4 di platform ini",
    description: "Peta praktik ITIL4 ke fitur platform dan bagaimana semuanya saling terhubung.",
    group: "itil4",
    icon: ConnectionsOutline,
    component: lazy(() => import("./content/itil4-overview")),
  },
  {
    slug: "project-settings",
    title: "Pengaturan project",
    description: "Ruang konfigurasi project: dari nama sampai automasi — khusus admin project.",
    group: "settings",
    icon: SettingsOutline,
    component: lazy(() => import("./content/project-settings")),
  },
];

export const getTopicBySlug = (slug: string): TDocTopic | undefined => DOC_TOPICS.find((topic) => topic.slug === slug);

export const getTopicNeighbors = (slug: string): { prev?: TDocTopic; next?: TDocTopic } => {
  const index = DOC_TOPICS.findIndex((topic) => topic.slug === slug);
  if (index === -1) return {};
  return { prev: DOC_TOPICS[index - 1], next: DOC_TOPICS[index + 1] };
};
