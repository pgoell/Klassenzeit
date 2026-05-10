import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  createMemoryHistory,
  createRootRouteWithContext,
  createRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { Toaster } from "@/components/ui/sonner";
import i18n from "@/i18n/init";
import {
  initialLessons,
  initialRooms,
  initialSchoolClasses,
  initialSubjects,
  scheduleByClassId,
  timeBlocksBySchemeId,
  violationsByClassId,
} from "../../../tests/msw-handlers";
import { SchedulePage } from "./schedule-page";

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

beforeEach(() => {
  for (const key of Object.keys(scheduleByClassId)) delete scheduleByClassId[key];
  for (const key of Object.keys(violationsByClassId)) delete violationsByClassId[key];
});

function renderSchedulePage(initialPath: string) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const rootRoute = createRootRouteWithContext<{ queryClient: QueryClient }>()({});
  const scheduleRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/schedule",
    component: () => <SchedulePage />,
    validateSearch: (search: Record<string, unknown>) => ({
      view:
        search.view === "class" || search.view === "teacher" || search.view === "room"
          ? search.view
          : undefined,
      class: typeof search.class === "string" ? search.class : undefined,
      teacher: typeof search.teacher === "string" ? search.teacher : undefined,
      room: typeof search.room === "string" ? search.room : undefined,
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

describe("SchedulePage", () => {
  it("renders the pick-a-class empty state when no class is selected", async () => {
    renderSchedulePage("/schedule");
    expect(await screen.findByText(/pick a class to view its schedule/i)).toBeInTheDocument();
  });

  it("renders the teacher view when ?view=teacher is in the URL", async () => {
    renderSchedulePage("/schedule?view=teacher");
    expect(
      await screen.findByText(/select a teacher above to see their weekly placements/i),
    ).toBeInTheDocument();
  });

  it("renders the Generate CTA when the selected class has no placements", async () => {
    const schoolClass = initialSchoolClasses[0];
    if (!schoolClass) throw new Error("seed missing");
    scheduleByClassId[schoolClass.id] = [];
    renderSchedulePage(`/schedule?class=${schoolClass.id}`);
    await waitFor(() =>
      expect(screen.getAllByRole("button", { name: /generate schedule/i }).length).toBeGreaterThan(
        0,
      ),
    );
  });

  it("renders a cell for each placement with resolved subject + room labels", async () => {
    const schoolClass = initialSchoolClasses[0];
    const lesson = initialLessons[0];
    const room = initialRooms[0];
    const subject = initialSubjects[0];
    if (!schoolClass || !lesson || !room || !subject) throw new Error("seed missing");
    timeBlocksBySchemeId[schoolClass.week_scheme_id] = [
      {
        id: "tb-mon-1",
        day_of_week: 0,
        position: 1,
        start_time: "08:00:00",
        end_time: "08:45:00",
      },
    ];
    scheduleByClassId[schoolClass.id] = [
      {
        lesson_id: lesson.id,
        teacher_id: "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
        time_block_id: "tb-mon-1",
        room_id: room.id,
        pinned: false,
      },
    ];
    renderSchedulePage(`/schedule?class=${schoolClass.id}`);
    await waitFor(() => expect(screen.getByText(subject.name)).toBeInTheDocument());
    expect(screen.getByText(/Raum 101/)).toBeInTheDocument();
  });
});
