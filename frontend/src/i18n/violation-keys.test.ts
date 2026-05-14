import { describe, expect, it } from "vitest";
import { violationItemKey } from "./violation-keys";

describe("violationItemKey", () => {
  it("maps every kind to its typed i18n key", () => {
    expect(violationItemKey("no_qualified_teacher")).toBe("schedule.violations.noQualifiedTeacher");
    expect(violationItemKey("teacher_over_capacity")).toBe(
      "schedule.violations.teacherOverCapacity",
    );
    expect(violationItemKey("no_free_time_block")).toBe("schedule.violations.noFreeTimeBlock");
    expect(violationItemKey("no_suitable_room")).toBe("schedule.violations.noSuitableRoom");
  });

  it("maps supervision_gap to the supervisionGap key", () => {
    expect(violationItemKey("supervision_gap")).toBe("schedule.violations.supervisionGap");
  });
});
