import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "@/components/ui/sonner";
import i18n from "@/i18n/init";
import { scheduleByClassId, violationsByClassId } from "../../../tests/msw-handlers";
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

  it("renders 'Generate all' button and posts to /api/schedule/all on click", async () => {
    scheduleByClassId.c1 = [
      {
        lesson_id: "00000000-0000-0000-0000-00000000b001",
        time_block_id: "00000000-0000-0000-0000-00000000c001",
        room_id: "00000000-0000-0000-0000-00000000d001",
      },
    ];
    render(
      wrapToolbar(
        <ScheduleToolbar
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

    const button = screen.getByRole("button", { name: /generate all/i });
    await userEvent.click(button);

    await waitFor(() => {
      expect(screen.getByText(/Generated 1 classes/i)).toBeInTheDocument();
    });
  });
});
