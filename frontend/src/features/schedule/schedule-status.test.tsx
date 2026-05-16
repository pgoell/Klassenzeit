import { render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";
import type { Lesson } from "@/features/lessons/hooks";
import i18n from "@/i18n/init";
import type { Violation } from "./hooks";
import { ScheduleStatus } from "./schedule-status";

beforeAll(() => {
  void i18n.changeLanguage("en");
});

const LESSON_ID = "11111111-1111-1111-1111-111111111111";

const lesson: Lesson = {
  id: LESSON_ID,
  subject: {
    id: "22222222-2222-2222-2222-222222222222",
    name: "Mathematik",
    short_name: "M",
  },
  teacher: {
    id: "33333333-3333-3333-3333-333333333333",
    first_name: "Anna",
    last_name: "Müller",
    short_code: "MUE",
  },
  school_classes: [
    {
      id: "44444444-4444-4444-4444-444444444444",
      name: "1a",
    },
  ],
  hours_per_week: 5,
  preferred_block_size: 1,
  pre_buffer_minutes: 0,
  post_buffer_minutes: 0,
  lesson_group_id: null,
  created_at: "2026-04-25T00:00:00Z",
  updated_at: "2026-04-25T00:00:00Z",
};

const lessonById = new Map([[LESSON_ID, lesson]]);

function renderWithViolations(violations: Violation[]) {
  return render(
    <ScheduleStatus
      placementsCount={0}
      expectedHours={5}
      violations={violations}
      lessonById={lessonById}
    />,
  );
}

describe("ScheduleStatus typed violations", () => {
  it("renders no_qualified_teacher copy", () => {
    renderWithViolations([{ kind: "no_qualified_teacher", lesson_id: LESSON_ID, hour_index: 0 }]);
    expect(
      screen.getByText("Mathematik (hour 1): Müller is not qualified for this subject."),
    ).toBeInTheDocument();
  });

  it("renders teacher_over_capacity copy", () => {
    renderWithViolations([{ kind: "teacher_over_capacity", lesson_id: LESSON_ID, hour_index: 1 }]);
    expect(
      screen.getByText("Mathematik (hour 2): Müller would exceed their max weekly hours."),
    ).toBeInTheDocument();
  });

  it("renders no_free_time_block copy", () => {
    renderWithViolations([{ kind: "no_free_time_block", lesson_id: LESSON_ID, hour_index: 0 }]);
    expect(
      screen.getByText("Mathematik (hour 1): no free slot for Müller and 1a."),
    ).toBeInTheDocument();
  });

  it("renders no_suitable_room copy", () => {
    renderWithViolations([{ kind: "no_suitable_room", lesson_id: LESSON_ID, hour_index: 0 }]);
    expect(
      screen.getByText("Mathematik (hour 1): no suitable room is available."),
    ).toBeInTheDocument();
  });

  it("falls back to deleted-lesson copy when lesson is missing", () => {
    renderWithViolations([
      {
        kind: "no_qualified_teacher",
        lesson_id: "ffffffff-ffff-ffff-ffff-ffffffffffff",
        hour_index: 0,
      },
    ]);
    expect(screen.getByText(/\(hour 1\)/)).toBeInTheDocument();
  });
});
