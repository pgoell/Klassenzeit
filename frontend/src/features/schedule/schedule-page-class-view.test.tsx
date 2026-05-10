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
import { afterAll, beforeAll, beforeEach, describe, expect, test } from "vitest";
import { Toaster } from "@/components/ui/sonner";
import i18n from "@/i18n/init";
import { initialSchoolClasses, initialTeachers, server } from "../../../tests/msw-handlers";
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
