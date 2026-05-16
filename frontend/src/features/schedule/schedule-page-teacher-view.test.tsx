import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  createMemoryHistory,
  createRootRouteWithContext,
  createRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { Toaster } from "@/components/ui/sonner";
import i18n from "@/i18n/init";
import {
  initialSchoolClasses,
  qualityReportByTeacherId,
  server,
  teacherSchedulesByTeacherId,
} from "../../../tests/msw-handlers";
import { renderWithProviders } from "../../../tests/render-helpers";
import { SchedulePageTeacherView } from "./schedule-page-teacher-view";

const BASE = "http://localhost:3000";

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

beforeEach(() => {
  for (const k of Object.keys(teacherSchedulesByTeacherId)) {
    delete teacherSchedulesByTeacherId[k];
  }
});

describe("SchedulePageTeacherView", () => {
  it("renders the placeholder body when no teacher is selected", async () => {
    renderWithProviders(<SchedulePageTeacherView />);
    expect(await screen.findByText(/select a teacher above/i)).toBeInTheDocument();
  });
});

function renderTeacherView(initialPath: string) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const rootRoute = createRootRouteWithContext<{ queryClient: QueryClient }>()({});
  const scheduleRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/schedule",
    component: () => <SchedulePageTeacherView />,
    validateSearch: (search: Record<string, unknown>) => ({
      teacher: typeof search.teacher === "string" ? search.teacher : undefined,
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

describe("SchedulePageTeacherView supervision badge", () => {
  const SCHOOL_CLASS = initialSchoolClasses[0];
  if (!SCHOOL_CLASS) throw new Error("seed missing: initialSchoolClasses[0]");
  const SCHEME_ID = SCHOOL_CLASS.week_scheme_id;

  const TEACHER_ID = "11111111-1111-1111-1111-111111111111";
  const BREAK_A = "22222222-2222-2222-2222-222222222222"; // supervised
  const BREAK_B = "33333333-3333-3333-3333-333333333333"; // not supervised
  const LESSON_BLOCK = "44444444-4444-4444-4444-444444444444";
  const LESSON_ID = "55555555-5555-5555-5555-555555555555";
  const ROOM_ID = "66666666-6666-6666-6666-666666666666";

  afterAll(() => {
    server.resetHandlers();
  });

  it("renders supervision badge on break cells when current teacher is supervisor", async () => {
    server.use(
      http.get(`${BASE}/api/week-schemes/${SCHEME_ID}`, () =>
        HttpResponse.json({
          id: SCHEME_ID,
          name: "Standardwoche",
          description: "",
          time_blocks: [
            {
              id: LESSON_BLOCK,
              day_of_week: 0,
              position: 1,
              start_time: "08:00",
              end_time: "08:45",
              kind: "lesson",
            },
            {
              id: BREAK_A,
              day_of_week: 0,
              position: 3,
              start_time: "09:30",
              end_time: "09:45",
              kind: "break",
            },
            {
              id: BREAK_B,
              day_of_week: 0,
              position: 6,
              start_time: "11:30",
              end_time: "11:45",
              kind: "break",
            },
          ],
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        }),
      ),
      http.get(`${BASE}/api/lessons`, () =>
        HttpResponse.json([
          {
            id: LESSON_ID,
            school_classes: [{ id: SCHOOL_CLASS.id, name: "1a" }],
            subject: { id: "subj-1", name: "Mathematik", short_name: "MA" },
            teacher: {
              id: TEACHER_ID,
              first_name: "Anna",
              last_name: "Schmidt",
              short_code: "SCH",
            },
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: null,
            created_at: "2026-04-20T00:00:00Z",
            updated_at: "2026-04-20T00:00:00Z",
          },
        ]),
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
      http.get(`${BASE}/api/teachers/${TEACHER_ID}/schedule`, () =>
        HttpResponse.json({
          placements: [
            {
              lesson_id: LESSON_ID,
              teacher_id: TEACHER_ID,
              time_block_id: LESSON_BLOCK,
              room_id: ROOM_ID,
              pinned: false,
            },
          ],
          supervision_assignments: [{ time_block_id: BREAK_A, teacher_id: TEACHER_ID }],
        }),
      ),
    );

    renderTeacherView(`/schedule?teacher=${TEACHER_ID}`);

    const supervisedCells = await screen.findAllByText(/Supervision/);
    expect(supervisedCells.length).toBe(1);
  });
});

describe("SchedulePageTeacherView quality metrics", () => {
  const SCHOOL_CLASS = initialSchoolClasses[0];
  if (!SCHOOL_CLASS) throw new Error("seed missing: initialSchoolClasses[0]");
  const SCHEME_ID = SCHOOL_CLASS.week_scheme_id;

  const TEACHER_ID = "77777777-7777-7777-7777-777777777777";
  const LESSON_BLOCK = "88888888-8888-8888-8888-888888888888";
  const LESSON_ID = "99999999-9999-9999-9999-999999999999";
  const ROOM_ID = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

  afterAll(() => {
    server.resetHandlers();
  });

  it("renders the quality metrics section when teacher has gap hours", async () => {
    qualityReportByTeacherId[TEACHER_ID] = {
      hard_violations: 0,
      unplaced_hours: 0,
      class_gap_hours: 0,
      class_gap_hours_by_class: {},
      teacher_gap_hours: 2,
      teacher_gap_hours_by_teacher: { [TEACHER_ID]: 2 },
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
    server.use(
      http.get(`${BASE}/api/week-schemes/${SCHEME_ID}`, () =>
        HttpResponse.json({
          id: SCHEME_ID,
          name: "Standardwoche",
          description: "",
          time_blocks: [
            {
              id: LESSON_BLOCK,
              day_of_week: 0,
              position: 1,
              start_time: "08:00",
              end_time: "08:45",
              kind: "lesson",
            },
          ],
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        }),
      ),
      http.get(`${BASE}/api/lessons`, () =>
        HttpResponse.json([
          {
            id: LESSON_ID,
            school_classes: [{ id: SCHOOL_CLASS.id, name: "1a" }],
            subject: { id: "subj-1", name: "Mathematik", short_name: "MA" },
            teacher: {
              id: TEACHER_ID,
              first_name: "Anna",
              last_name: "Schmidt",
              short_code: "SCH",
            },
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: null,
            created_at: "2026-04-20T00:00:00Z",
            updated_at: "2026-04-20T00:00:00Z",
          },
        ]),
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
      http.get(`${BASE}/api/teachers/${TEACHER_ID}/schedule`, () =>
        HttpResponse.json({
          placements: [
            {
              lesson_id: LESSON_ID,
              teacher_id: TEACHER_ID,
              time_block_id: LESSON_BLOCK,
              room_id: ROOM_ID,
              pinned: false,
            },
          ],
          supervision_assignments: [],
          quality_issues: [],
          quality_report: qualityReportByTeacherId[TEACHER_ID],
        }),
      ),
    );

    renderTeacherView(`/schedule?teacher=${TEACHER_ID}`);
    expect(await screen.findByText("Quality metrics")).toBeInTheDocument();
    expect(screen.getByText("2 teacher gap hours")).toBeInTheDocument();
  });
});
