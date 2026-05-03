import { useNavigate, useSearch } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { useLessons } from "@/features/lessons/hooks";
import { useRooms } from "@/features/rooms/hooks";
import { useSchoolClasses } from "@/features/school-classes/hooks";
import { useTeachers } from "@/features/teachers/hooks";
import { useWeekSchemeDetail } from "@/features/week-schemes/hooks";
import { useTeacherSchedule } from "./hooks";
import { type ScheduleCell, ScheduleGrid } from "./schedule-grid";
import { ScheduleToolbar } from "./schedule-toolbar";

export function SchedulePageTeacherView() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const search = useSearch({ strict: false }) as { teacher?: string };
  const teacherId = search.teacher;

  const teachers = useTeachers();
  const lessons = useLessons();
  const rooms = useRooms();
  const classes = useSchoolClasses();
  // Sprint B simplification: assume every class shares one week scheme. Pull
  // its time blocks from the first class's scheme. Multi-scheme schools are a
  // follow-up.
  const firstSchemeId = classes.data?.[0]?.week_scheme_id ?? null;
  const weekScheme = useWeekSchemeDetail(firstSchemeId);
  const schedule = useTeacherSchedule(teacherId);

  function onTeacherChange(id: string) {
    void navigate({ to: "/schedule", search: { view: "teacher", teacher: id } });
  }

  if (!teacherId) {
    return (
      <div className="space-y-4">
        <ScheduleToolbar
          view="teacher"
          teachers={teachers.data ?? []}
          teacherId={undefined}
          onTeacherChange={onTeacherChange}
        />
        <p className="text-sm text-muted-foreground">{t("schedule.empty.teacherBody")}</p>
      </div>
    );
  }

  const lessonById = new Map((lessons.data ?? []).map((l) => [l.id, l]));
  const roomById = new Map((rooms.data ?? []).map((r) => [r.id, r]));
  const blockById = new Map((weekScheme.data?.time_blocks ?? []).map((b) => [b.id, b]));

  const placements = schedule.data?.placements ?? [];
  const cells: ScheduleCell[] = placements
    .map((p): ScheduleCell | undefined => {
      const lesson = lessonById.get(p.lesson_id);
      const block = blockById.get(p.time_block_id);
      if (!lesson || !block) return undefined;
      const room = roomById.get(p.room_id);
      const classNames = lesson.school_classes.map((c) => c.name).join(", ");
      return {
        key: `${block.day_of_week}:${block.position}`,
        day: block.day_of_week,
        position: block.position,
        subjectName: lesson.subject.name,
        classNames,
        roomName: room?.name ?? t("schedule.cellDeletedLesson"),
      };
    })
    .filter((c): c is ScheduleCell => c !== undefined);

  const daysPresent = Array.from(new Set(cells.map((c) => c.day))).sort((a, b) => a - b);
  const positions = Array.from(new Set(cells.map((c) => c.position))).sort((a, b) => a - b);

  return (
    <div className="space-y-5">
      <ScheduleToolbar
        view="teacher"
        teachers={teachers.data ?? []}
        teacherId={teacherId}
        onTeacherChange={onTeacherChange}
      />
      {schedule.isLoading ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : placements.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("schedule.empty.body")}</p>
      ) : (
        <ScheduleGrid cells={cells} daysPresent={daysPresent} positions={positions} />
      )}
    </div>
  );
}
