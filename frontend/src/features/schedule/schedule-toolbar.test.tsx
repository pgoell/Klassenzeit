import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import type { ReactNode } from "react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "@/components/ui/sonner";
import i18n from "@/i18n/init";
import { scheduleByClassId, server, violationsByClassId } from "../../../tests/msw-handlers";
import { ScheduleToolbar } from "./schedule-toolbar";

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

const CLASSES = [
  {
    id: "c1",
    name: "1a",
    grade_level: 1,
    stundentafel_id: "st1",
    week_scheme_id: "ws1",
    created_at: "2026-04-17T00:00:00Z",
    updated_at: "2026-04-17T00:00:00Z",
  },
  {
    id: "c2",
    name: "2b",
    grade_level: 2,
    stundentafel_id: "st1",
    week_scheme_id: "ws1",
    created_at: "2026-04-17T00:00:00Z",
    updated_at: "2026-04-17T00:00:00Z",
  },
];

function wrapToolbar(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return (
    <QueryClientProvider client={queryClient}>
      {children}
      <Toaster />
    </QueryClientProvider>
  );
}

describe("ScheduleToolbar", () => {
  beforeEach(() => {
    for (const key of Object.keys(scheduleByClassId)) delete scheduleByClassId[key];
    for (const key of Object.keys(violationsByClassId)) delete violationsByClassId[key];
  });

  it("renders the Generate button and calls onGenerate when clicked with no placements", () => {
    const onGenerate = vi.fn();
    render(
      wrapToolbar(
        <ScheduleToolbar
          view="class"
          classes={CLASSES}
          classId="c1"
          onClassChange={vi.fn()}
          onGenerate={onGenerate}
          onCancelConfirm={vi.fn()}
          placementsCount={0}
          confirming={false}
          pending={false}
        />,
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: /generate schedule/i }));
    expect(onGenerate).toHaveBeenCalledTimes(1);
  });

  it("renders the replace banner when confirming is true", () => {
    render(
      wrapToolbar(
        <ScheduleToolbar
          view="class"
          classes={CLASSES}
          classId="c1"
          onClassChange={vi.fn()}
          onGenerate={vi.fn()}
          onCancelConfirm={vi.fn()}
          placementsCount={18}
          confirming={true}
          pending={false}
        />,
      ),
    );
    expect(screen.getByText(/will replace 18 placements/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /generate anyway/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^cancel$/i })).toBeInTheDocument();
  });

  it("disables the Generate button while pending and shows the saving label", () => {
    render(
      wrapToolbar(
        <ScheduleToolbar
          view="class"
          classes={CLASSES}
          classId="c1"
          onClassChange={vi.fn()}
          onGenerate={vi.fn()}
          onCancelConfirm={vi.fn()}
          placementsCount={0}
          confirming={false}
          pending={true}
        />,
      ),
    );
    const button = screen.getByRole("button", { name: /saving/i });
    expect(button).toBeDisabled();
  });

  it("'Re-solve respecting my pins' posts respect_pins=true and toasts the pins-preserved copy", async () => {
    const requestUrls: string[] = [];
    server.use(
      http.post("http://localhost:3000/api/schedule/all", ({ request }) => {
        requestUrls.push(request.url);
        return HttpResponse.json({
          classes: [{ class_id: "c1", placements_count: 1, violations_count: 0 }],
          total_placements: 1,
          total_violations: 0,
        });
      }),
    );
    render(
      wrapToolbar(
        <ScheduleToolbar
          view="class"
          classes={CLASSES}
          classId="c1"
          onClassChange={vi.fn()}
          onGenerate={vi.fn()}
          onCancelConfirm={vi.fn()}
          placementsCount={0}
          confirming={false}
          pending={false}
        />,
      ),
    );

    const button = screen.getByRole("button", { name: /re-solve respecting my pins/i });
    await userEvent.click(button);

    await waitFor(() => {
      expect(screen.getByText(/your pins were preserved/i)).toBeInTheDocument();
    });
    expect(requestUrls).toHaveLength(1);
    expect(requestUrls[0]).toContain("respect_pins=true");
  });

  it("'Generate all from scratch' posts respect_pins=false and toasts the from-scratch copy", async () => {
    const requestUrls: string[] = [];
    server.use(
      http.post("http://localhost:3000/api/schedule/all", ({ request }) => {
        requestUrls.push(request.url);
        return HttpResponse.json({
          classes: [{ class_id: "c1", placements_count: 1, violations_count: 0 }],
          total_placements: 1,
          total_violations: 0,
        });
      }),
    );
    render(
      wrapToolbar(
        <ScheduleToolbar
          view="class"
          classes={CLASSES}
          classId="c1"
          onClassChange={vi.fn()}
          onGenerate={vi.fn()}
          onCancelConfirm={vi.fn()}
          placementsCount={0}
          confirming={false}
          pending={false}
        />,
      ),
    );

    const button = screen.getByRole("button", { name: /generate all from scratch/i });
    await userEvent.click(button);

    await waitFor(() => {
      expect(screen.getByText(/regenerated from scratch/i)).toBeInTheDocument();
    });
    expect(requestUrls).toHaveLength(1);
    expect(requestUrls[0]).toContain("respect_pins=false");
  });
});
