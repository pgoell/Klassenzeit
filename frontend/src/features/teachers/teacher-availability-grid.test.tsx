import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { beforeAll, describe, expect, it } from "vitest";
import i18n from "@/i18n/init";
import {
  initialTeachers,
  server,
  teacherAvailabilityByTeacherId,
  timeBlocksBySchemeId,
} from "../../../tests/msw-handlers";
import { renderWithProviders } from "../../../tests/render-helpers";
import { teacherDetailQueryKey } from "./hooks";
import { TeacherAvailabilityGrid } from "./teacher-availability-grid";

const teacherId = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const schemeId = "cccccccc-cccc-cccc-cccc-cccccccccccc";

describe("TeacherAvailabilityGrid", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("submits preferred and unavailable entries; omits available cells", async () => {
    timeBlocksBySchemeId[schemeId] = [
      {
        id: "tb-mon-1",
        day_of_week: 0,
        position: 1,
        start_time: "08:00:00",
        end_time: "08:45:00",
      },
      {
        id: "tb-mon-2",
        day_of_week: 0,
        position: 2,
        start_time: "08:50:00",
        end_time: "09:35:00",
      },
    ];
    teacherAvailabilityByTeacherId[teacherId] = [];
    const user = userEvent.setup();
    renderWithProviders(<TeacherAvailabilityGrid teacherId={teacherId} />);

    const preferredButtons = await screen.findAllByRole("button", {
      name: /^preferred/i,
    });
    const firstPreferred = preferredButtons[0];
    if (!firstPreferred) throw new Error("missing preferred button");
    await user.click(firstPreferred);

    const unavailableButtons = await screen.findAllByRole("button", {
      name: /^unavailable/i,
    });
    const secondUnavailable = unavailableButtons[1];
    if (!secondUnavailable) throw new Error("missing second unavailable button");
    await user.click(secondUnavailable);

    await user.click(screen.getByRole("button", { name: /save availability/i }));

    expect(teacherAvailabilityByTeacherId[teacherId]).toEqual([
      { time_block_id: "tb-mon-1", status: "preferred" },
      { time_block_id: "tb-mon-2", status: "unavailable" },
    ]);
  });

  it("greys columns whose day is not in working_days", async () => {
    timeBlocksBySchemeId[schemeId] = [
      {
        id: "tb-mon-1",
        day_of_week: 0,
        position: 1,
        start_time: "08:00:00",
        end_time: "08:45:00",
      },
      {
        id: "tb-wed-1",
        day_of_week: 2,
        position: 1,
        start_time: "08:00:00",
        end_time: "08:45:00",
      },
    ];
    teacherAvailabilityByTeacherId[teacherId] = [];
    const base = initialTeachers[0];
    if (!base) throw new Error("missing initial teacher fixture");
    server.use(
      http.get(`http://localhost:3000/api/teachers/${teacherId}`, () =>
        HttpResponse.json({
          ...base,
          id: teacherId,
          qualifications: [],
          availability: [],
          working_days: [0, 1],
        }),
      ),
    );

    renderWithProviders(<TeacherAvailabilityGrid teacherId={teacherId} />);
    const monHeader = await screen.findByRole("columnheader", { name: "Mon" });
    const wedHeader = await screen.findByRole("columnheader", { name: "Wed" });
    expect(monHeader).not.toHaveAttribute("data-off-day", "true");
    expect(wedHeader).toHaveAttribute("data-off-day", "true");
  });

  it("preserves preferred markers when the detail query refetches in the background", async () => {
    timeBlocksBySchemeId[schemeId] = [
      {
        id: "tb-mon-1",
        day_of_week: 0,
        position: 1,
        start_time: "08:00:00",
        end_time: "08:45:00",
      },
    ];
    teacherAvailabilityByTeacherId[teacherId] = [];
    const user = userEvent.setup();
    const { queryClient } = renderWithProviders(<TeacherAvailabilityGrid teacherId={teacherId} />);

    const preferredButtons = await screen.findAllByRole("button", { name: /^Preferred/i });
    const firstPreferred = preferredButtons[0];
    if (!firstPreferred) throw new Error("missing preferred button");
    await user.click(firstPreferred);
    expect(firstPreferred).toHaveAttribute("aria-pressed", "true");

    // Simulate a sibling-tab change: a different availability set was persisted,
    // then the detail query is invalidated to trigger a background refetch.
    teacherAvailabilityByTeacherId[teacherId] = [
      { time_block_id: "tb-some-other-block", status: "unavailable" },
    ];
    await queryClient.invalidateQueries({ queryKey: teacherDetailQueryKey(teacherId) });

    // Wait for the refetch to settle.
    await screen.findAllByRole("button", { name: /^Preferred/i });

    // The user's in-progress preferred toggle must survive the background refetch.
    const preferredAfter = screen.getAllByRole("button", { name: /^Preferred/i });
    const firstPreferredAfter = preferredAfter[0];
    if (!firstPreferredAfter) throw new Error("missing preferred button after refetch");
    expect(firstPreferredAfter).toHaveAttribute("aria-pressed", "true");
  });
});
