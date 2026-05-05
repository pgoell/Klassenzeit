import type { components } from "@/lib/api-types";

type ViolationKind = components["schemas"]["ViolationResponse"]["kind"];

export function violationItemKey(
  kind: ViolationKind,
):
  | "schedule.violations.noQualifiedTeacher"
  | "schedule.violations.teacherOverCapacity"
  | "schedule.violations.noFreeTimeBlock"
  | "schedule.violations.noSuitableRoom"
  | "schedule.violations.lessonGroupSplit"
  | "schedule.violations.pinnedConflict"
  | "schedule.violations.subjectDailyHourCapExceeded"
  | "schedule.violations.classDailyLessonCapExceeded" {
  switch (kind) {
    case "no_qualified_teacher":
      return "schedule.violations.noQualifiedTeacher";
    case "teacher_over_capacity":
      return "schedule.violations.teacherOverCapacity";
    case "no_free_time_block":
      return "schedule.violations.noFreeTimeBlock";
    case "no_suitable_room":
      return "schedule.violations.noSuitableRoom";
    case "lesson_group_split":
      return "schedule.violations.lessonGroupSplit";
    case "pinned_conflict":
      return "schedule.violations.pinnedConflict";
    case "subject_daily_hour_cap_exceeded":
      return "schedule.violations.subjectDailyHourCapExceeded";
    case "class_daily_lesson_cap_exceeded":
      return "schedule.violations.classDailyLessonCapExceeded";
  }
}
