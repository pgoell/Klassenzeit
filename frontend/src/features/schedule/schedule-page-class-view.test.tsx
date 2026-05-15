import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  createMemoryHistory,
  createRootRouteWithContext,
  createRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { afterAll, beforeAll, beforeEach, describe, expect, test } from "vitest";
import { Toaster } from "@/components/ui/sonner";
import i18n from "@/i18n/init";
import type { components } from "@/lib/api-types";
import {
  initialSchoolClasses,
  initialTeachers,
  qualityReportByClassId,
  server,
} from "../../../tests/msw-handlers";
import { SchedulePageClassView } from "./schedule-page-class-view";

const BASE = "http://localhost:3000";
const SCHOOL_CLASS = initialSchoolClasses[0];
if (!SCHOOL_CLASS) throw new Error("seed missing: initialSchoolClasses[0]");
const TEACHER = initialTeachers[0];
if (!TEACHER) throw new Error("seed missing: initialTeachers[0]");
const CLASS_ID = SCHOOL_CLASS.id;
const SCHEME_ID = SCHOOL_CLASS.week_scheme_id;
const SUBJECT_ID = "11111111-1111-1111-1111-111111111111";
const LESSON_ID = "22222222-2222-2222-2222-222222222222";
const TIME_BLOCK_ID = "33333333-3333-3333-3333-333333333333";
const ROOM_ID = "44444444-4444-4444-4444-444444444444";

function renderClassView(initialPath: string) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const rootRoute = createRootRouteWithContext<{ queryClient: QueryClient }>()({});
  const scheduleRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/schedule",
    component: () => <SchedulePageClassView />,
    validateSearch: (search: Record<string, unknown>) => ({
      class: typeof search.class === "string" ? search.class : undefined,
    }),
  });
  const router = createRouter({
    routeTree: rootRoute.addChildren([scheduleRoute]),
    history: createMemoryHistory({ initialEntries: [initialPath] }),
    context: { queryClient },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
      <Toaster />
    </QueryClientProvider>,
  );
}

describe("SchedulePageClassView teacher resolution", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  beforeEach(() => {
    server.use(
      http.get(`${BASE}/api/lessons`, () =>
        HttpResponse.json([
          {
            id: LESSON_ID,
            school_classes: [{ id: CLASS_ID, name: "1a" }],
            subject: { id: SUBJECT_ID, name: "Mathematik", short_name: "MA" },
            teacher: null,
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: null,
            created_at: "2026-04-20T00:00:00Z",
            updated_at: "2026-04-20T00:00:00Z",
          },
        ]),
      ),
      http.get(`${BASE}/api/week-schemes/${SCHEME_ID}`, () =>
        HttpResponse.json({
          id: SCHEME_ID,
          name: "Standardwoche",
          description: "",
          time_blocks: [
            {
              id: TIME_BLOCK_ID,
              day_of_week: 1,
              position: 1,
              start_time: "08:00",
              end_time: "08:45",
            },
          ],
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        }),
      ),
      http.get(`${BASE}/api/classes/${CLASS_ID}/schedule`, () =>
        HttpResponse.json({
          placements: [
            {
              lesson_id: LESSON_ID,
              teacher_id: TEACHER.id,
              time_block_id: TIME_BLOCK_ID,
              room_id: ROOM_ID,
              pinned: false,
            },
          ],
          violations: [],
        }),
      ),
      http.get(`${BASE}/api/rooms`, () =>
        HttpResponse.json([
          {
            id: ROOM_ID,
            name: "R1",
            short_name: "R1",
            capacity: 30,
            created_at: "2026-04-17T00:00:00Z",
            updated_at: "2026-04-17T00:00:00Z",
          },
        ]),
      ),
    );
  });
  afterAll(() => {
    server.resetHandlers();
  });

  test("cell renders solver-picked teacher when lesson is unpinned", async () => {
    renderClassView(`/schedule?class=${CLASS_ID}`);
    expect(await screen.findByText(new RegExp(TEACHER.last_name))).toBeVisible();
  });
});

describe("SchedulePageClassView class header band", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  afterAll(() => {
    server.resetHandlers();
  });

  const TAFEL_ID = SCHOOL_CLASS.stundentafel_id;
  const SUBJ_MATH = "11111111-1111-1111-1111-111111111111";
  const SUBJ_GERMAN = "11111111-1111-1111-1111-111111111112";
  const SUBJ_ART = "11111111-1111-1111-1111-111111111113";
  const SUBJ_MUSIC = "11111111-1111-1111-1111-111111111114";
  const KL_ID = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
  const KL_FIRST = "Maria";
  const KL_LAST = "Müller";

  function emptyTafelEntries() {
    return [];
  }

  function fourSubjectTafelEntries() {
    return [
      {
        id: "e1",
        subject: { id: SUBJ_MATH, name: "Mathematik", short_name: "MA" },
        hours_per_week: 4,
        preferred_block_size: 1,
      },
      {
        id: "e2",
        subject: { id: SUBJ_GERMAN, name: "Deutsch", short_name: "DE" },
        hours_per_week: 4,
        preferred_block_size: 1,
      },
      {
        id: "e3",
        subject: { id: SUBJ_ART, name: "Kunst", short_name: "KU" },
        hours_per_week: 2,
        preferred_block_size: 1,
      },
      {
        id: "e4",
        subject: { id: SUBJ_MUSIC, name: "Musik", short_name: "MU" },
        hours_per_week: 1,
        preferred_block_size: 1,
      },
    ];
  }

  function emptyScheduleHandlers() {
    return [
      http.get(`${BASE}/api/lessons`, () => HttpResponse.json([])),
      http.get(`${BASE}/api/week-schemes/${SCHEME_ID}`, () =>
        HttpResponse.json({
          id: SCHEME_ID,
          name: "Standardwoche",
          description: "",
          time_blocks: [],
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        }),
      ),
      http.get(`${BASE}/api/classes/${CLASS_ID}/schedule`, () =>
        HttpResponse.json({ placements: [], violations: [] }),
      ),
      http.get(`${BASE}/api/rooms`, () => HttpResponse.json([])),
    ];
  }

  test("class header band shows Klassenlehrer name and coverage when set", async () => {
    server.use(
      ...emptyScheduleHandlers(),
      http.get(`${BASE}/api/classes`, () =>
        HttpResponse.json([{ ...SCHOOL_CLASS, class_teacher_id: KL_ID }]),
      ),
      http.get(`${BASE}/api/teachers`, () =>
        HttpResponse.json([
          {
            id: KL_ID,
            first_name: KL_FIRST,
            last_name: KL_LAST,
            short_code: "MUE",
            max_hours_per_week: 25,
            is_active: true,
            subject_ids: [SUBJ_MATH, SUBJ_GERMAN],
            created_at: "2026-04-17T00:00:00Z",
            updated_at: "2026-04-17T00:00:00Z",
          },
        ]),
      ),
      http.get(`${BASE}/api/stundentafeln/${TAFEL_ID}`, () =>
        HttpResponse.json({
          id: TAFEL_ID,
          name: "Grundschule Klasse 1",
          grade_level: 1,
          school_type: "Grundschule",
          entries: fourSubjectTafelEntries(),
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        }),
      ),
    );
    renderClassView(`/schedule?class=${CLASS_ID}`);
    expect(await screen.findByText(`${KL_FIRST} ${KL_LAST}`)).toBeVisible();
    expect(await screen.findByText("Class teacher:")).toBeVisible();
    expect(await screen.findByText("covers 2 of 4 subjects")).toBeVisible();
  });

  test("class header band shows 'not assigned' when class_teacher_id is null", async () => {
    server.use(
      ...emptyScheduleHandlers(),
      http.get(`${BASE}/api/classes`, () =>
        HttpResponse.json([{ ...SCHOOL_CLASS, class_teacher_id: null }]),
      ),
      http.get(`${BASE}/api/teachers`, () => HttpResponse.json([])),
      http.get(`${BASE}/api/stundentafeln/${TAFEL_ID}`, () =>
        HttpResponse.json({
          id: TAFEL_ID,
          name: "Grundschule Klasse 1",
          grade_level: 1,
          school_type: "Grundschule",
          entries: fourSubjectTafelEntries(),
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        }),
      ),
    );
    renderClassView(`/schedule?class=${CLASS_ID}`);
    expect(await screen.findByText("not assigned")).toBeVisible();
    expect(screen.queryByText(/covers \d+ of \d+ subjects/)).toBeNull();
  });

  test("class header band omits coverage when the Stundentafel has zero entries", async () => {
    server.use(
      ...emptyScheduleHandlers(),
      http.get(`${BASE}/api/classes`, () =>
        HttpResponse.json([{ ...SCHOOL_CLASS, class_teacher_id: KL_ID }]),
      ),
      http.get(`${BASE}/api/teachers`, () =>
        HttpResponse.json([
          {
            id: KL_ID,
            first_name: KL_FIRST,
            last_name: KL_LAST,
            short_code: "MUE",
            max_hours_per_week: 25,
            is_active: true,
            subject_ids: [SUBJ_MATH],
            created_at: "2026-04-17T00:00:00Z",
            updated_at: "2026-04-17T00:00:00Z",
          },
        ]),
      ),
      http.get(`${BASE}/api/stundentafeln/${TAFEL_ID}`, () =>
        HttpResponse.json({
          id: TAFEL_ID,
          name: "Grundschule Klasse 1",
          grade_level: 1,
          school_type: "Grundschule",
          entries: emptyTafelEntries(),
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        }),
      ),
    );
    renderClassView(`/schedule?class=${CLASS_ID}`);
    expect(await screen.findByText(`${KL_FIRST} ${KL_LAST}`)).toBeVisible();
    expect(screen.queryByText(/covers \d+ of \d+ subjects/)).toBeNull();
  });
});

describe("SchedulePageClassView quality-issue sidebar", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  afterAll(() => {
    server.resetHandlers();
  });

  beforeEach(() => {
    server.use(
      http.get(`${BASE}/api/lessons`, () =>
        HttpResponse.json([
          {
            id: LESSON_ID,
            school_classes: [{ id: CLASS_ID, name: "1a" }],
            subject: { id: SUBJECT_ID, name: "Mathematik", short_name: "MA" },
            teacher: null,
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: null,
            created_at: "2026-04-20T00:00:00Z",
            updated_at: "2026-04-20T00:00:00Z",
          },
        ]),
      ),
      http.get(`${BASE}/api/week-schemes/${SCHEME_ID}`, () =>
        HttpResponse.json({
          id: SCHEME_ID,
          name: "Standardwoche",
          description: "",
          time_blocks: [
            {
              id: TIME_BLOCK_ID,
              day_of_week: 1,
              position: 1,
              start_time: "08:00",
              end_time: "08:45",
            },
          ],
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        }),
      ),
      http.get(`${BASE}/api/classes/${CLASS_ID}/schedule`, () =>
        HttpResponse.json({
          placements: [
            {
              lesson_id: LESSON_ID,
              teacher_id: TEACHER.id,
              time_block_id: TIME_BLOCK_ID,
              room_id: ROOM_ID,
              pinned: false,
            },
          ],
          violations: [],
          quality_issues: [
            {
              kind: "room_hop",
              school_class_id: CLASS_ID,
              day_of_week: 1,
              subject_id: SUBJECT_ID,
              detail: { rooms: ["r1", "r2"] },
              cells: [{ day_of_week: 1, position: 1 }],
            },
          ],
        }),
      ),
      http.get(`${BASE}/api/rooms`, () =>
        HttpResponse.json([
          {
            id: ROOM_ID,
            name: "R1",
            short_name: "R1",
            capacity: 30,
            created_at: "2026-04-17T00:00:00Z",
            updated_at: "2026-04-17T00:00:00Z",
          },
        ]),
      ),
    );
  });

  test("mounts the QualityIssueSidebar with issues from the GET response", async () => {
    renderClassView(`/schedule?class=${CLASS_ID}`);
    expect(await screen.findByText("Quality issues (1)")).toBeVisible();
    expect(screen.getByRole("button", { name: /Room hop/i })).toBeInTheDocument();
  });

  test("clicking an issue row highlights the matching grid cell", async () => {
    const { container } = renderClassView(`/schedule?class=${CLASS_ID}`);
    const button = await screen.findByRole("button", { name: /Room hop/i });
    await userEvent.click(button);
    await waitFor(() => {
      const highlighted = container.querySelector(
        '[data-cell-day="1"][data-cell-pos="1"][data-highlight="true"]',
      );
      expect(highlighted).not.toBeNull();
    });
    // The highlighted cell contains the placement subject name.
    const highlighted = container.querySelector(
      '[data-cell-day="1"][data-cell-pos="1"][data-highlight="true"]',
    );
    expect(highlighted).not.toBeNull();
    if (highlighted) {
      expect(within(highlighted as HTMLElement).getByText("Mathematik")).toBeInTheDocument();
    }
  });
});

const EMPTY_QUALITY_REPORT_FIXTURE: components["schemas"]["QualityReportResponse"] = {
  hard_violations: 0,
  unplaced_hours: 0,
  class_gap_hours: 0,
  class_gap_hours_by_class: {},
  teacher_gap_hours: 0,
  teacher_gap_hours_by_teacher: {},
  class_day_balance_cost: 0,
  class_day_balance_cost_by_class: {},
  home_room_misses: 0,
  home_room_misses_by_class: {},
  prefer_early_units: 0,
  avoid_first_units: 0,
  avoid_last_units: 0,
  prefer_late_units: 0,
  prefer_class_teacher_misses: 0,
  weighted_score: 0,
  worst_per_class_spread: 0,
  worst_per_class_interior_gaps: 0,
  soft_pin_misses: 0,
  supervision_spread_raw: 0,
};

describe("SchedulePageClassView quality_report wiring", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  afterAll(() => {
    server.resetHandlers();
  });

  beforeEach(() => {
    server.use(
      http.get(`${BASE}/api/lessons`, () =>
        HttpResponse.json([
          {
            id: LESSON_ID,
            school_classes: [{ id: CLASS_ID, name: "1a" }],
            subject: { id: SUBJECT_ID, name: "Mathematik", short_name: "MA" },
            teacher: null,
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: null,
            created_at: "2026-04-20T00:00:00Z",
            updated_at: "2026-04-20T00:00:00Z",
          },
        ]),
      ),
      http.get(`${BASE}/api/week-schemes/${SCHEME_ID}`, () =>
        HttpResponse.json({
          id: SCHEME_ID,
          name: "Standardwoche",
          description: "",
          time_blocks: [
            {
              id: TIME_BLOCK_ID,
              day_of_week: 1,
              position: 1,
              start_time: "08:00",
              end_time: "08:45",
            },
          ],
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        }),
      ),
      http.get(`${BASE}/api/classes/${CLASS_ID}/schedule`, () =>
        HttpResponse.json({
          placements: [
            {
              lesson_id: LESSON_ID,
              teacher_id: TEACHER.id,
              time_block_id: TIME_BLOCK_ID,
              room_id: ROOM_ID,
              pinned: false,
            },
          ],
          violations: [],
          quality_issues: [],
          quality_report: qualityReportByClassId[CLASS_ID] ?? null,
        }),
      ),
      http.post(`${BASE}/api/classes/${CLASS_ID}/schedule`, () =>
        HttpResponse.json({
          placements: [
            {
              lesson_id: LESSON_ID,
              teacher_id: TEACHER.id,
              time_block_id: TIME_BLOCK_ID,
              room_id: ROOM_ID,
              pinned: false,
            },
          ],
          violations: [],
          soft_score: 0,
          quality_report: qualityReportByClassId[CLASS_ID] ?? EMPTY_QUALITY_REPORT_FIXTURE,
          was_cancelled: false,
          supervision_assignments: [],
          quality_issues: [],
        }),
      ),
      http.get(`${BASE}/api/rooms`, () =>
        HttpResponse.json([
          {
            id: ROOM_ID,
            name: "R1",
            short_name: "R1",
            capacity: 30,
            created_at: "2026-04-17T00:00:00Z",
            updated_at: "2026-04-17T00:00:00Z",
          },
        ]),
      ),
    );
  });

  test("passes the POST quality_report attribution to the sidebar", async () => {
    qualityReportByClassId[CLASS_ID] = {
      ...EMPTY_QUALITY_REPORT_FIXTURE,
      class_gap_hours_by_class: { [CLASS_ID]: 4 },
      home_room_misses_by_class: { [CLASS_ID]: 2 },
    };
    renderClassView(`/schedule?class=${CLASS_ID}`);
    const generateButton = await screen.findByRole("button", { name: /^Generate schedule$/i });
    await userEvent.click(generateButton);
    // Existing placements trigger the confirm-replace banner; click through it.
    const confirmButton = await screen.findByRole("button", { name: /^Generate anyway$/i });
    await userEvent.click(confirmButton);
    expect(await screen.findByText("4 class gap hours")).toBeInTheDocument();
    expect(screen.getByText("2 home-room misses")).toBeInTheDocument();
  });

  test("falls back to GET quality_report on initial page load", async () => {
    qualityReportByClassId[CLASS_ID] = {
      ...EMPTY_QUALITY_REPORT_FIXTURE,
      class_gap_hours_by_class: { [CLASS_ID]: 1 },
      home_room_misses_by_class: {},
    };
    renderClassView(`/schedule?class=${CLASS_ID}`);
    expect(await screen.findByText("1 class gap hour")).toBeInTheDocument();
  });

  test("prefers POST quality_report when both POST and GET are present", async () => {
    // GET seeds 1 class gap hour; POST will seed 5 (different value to distinguish).
    qualityReportByClassId[CLASS_ID] = {
      ...EMPTY_QUALITY_REPORT_FIXTURE,
      class_gap_hours_by_class: { [CLASS_ID]: 1 },
    };
    renderClassView(`/schedule?class=${CLASS_ID}`);
    expect(await screen.findByText("1 class gap hour")).toBeInTheDocument();
    qualityReportByClassId[CLASS_ID] = {
      ...EMPTY_QUALITY_REPORT_FIXTURE,
      class_gap_hours_by_class: { [CLASS_ID]: 5 },
    };
    const generateButton = await screen.findByRole("button", { name: /^Generate schedule$/i });
    await userEvent.click(generateButton);
    // Existing placements trigger the confirm-replace banner; click through it.
    const confirmButton = await screen.findByRole("button", { name: /^Generate anyway$/i });
    await userEvent.click(confirmButton);
    expect(await screen.findByText("5 class gap hours")).toBeInTheDocument();
  });
});
