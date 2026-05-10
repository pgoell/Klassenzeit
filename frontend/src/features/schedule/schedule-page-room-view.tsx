import { useNavigate, useSearch } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { useLessons } from "@/features/lessons/hooks";
import { useRooms } from "@/features/rooms/hooks";
import { useSchoolClasses } from "@/features/school-classes/hooks";
import { useTeachers } from "@/features/teachers/hooks";
import { useWeekSchemeDetail } from "@/features/week-schemes/hooks";
import { useRoomSchedule } from "./hooks";
import { type ScheduleCell, ScheduleGrid } from "./schedule-grid";
import { ScheduleToolbar } from "./schedule-toolbar";

export function SchedulePageRoomView() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const search = useSearch({ strict: false }) as { room?: string };
  const roomId = search.room;

  const rooms = useRooms();
  const lessons = useLessons();
  const classes = useSchoolClasses();
  const teachers = useTeachers();
  // Sprint B simplification: assume every class shares one week scheme. Pull
  // its time blocks from the first class's scheme.
  const firstSchemeId = classes.data?.[0]?.week_scheme_id ?? null;
  const weekScheme = useWeekSchemeDetail(firstSchemeId);
  const schedule = useRoomSchedule(roomId);

  function onRoomChange(id: string) {
    void navigate({ to: "/schedule", search: { view: "room", room: id } });
  }

  if (!roomId) {
    return (
      <div className="space-y-4">
        <ScheduleToolbar
          view="room"
          rooms={rooms.data ?? []}
          roomId={undefined}
          onRoomChange={onRoomChange}
        />
        <p className="text-sm text-muted-foreground">{t("schedule.empty.roomBody")}</p>
      </div>
    );
  }

  const lessonById = new Map((lessons.data ?? []).map((l) => [l.id, l]));
  const blockById = new Map((weekScheme.data?.time_blocks ?? []).map((b) => [b.id, b]));
  const teacherById = new Map((teachers.data ?? []).map((t) => [t.id, t]));

  const placements = schedule.data?.placements ?? [];
  const cells: ScheduleCell[] = placements
    .map((p): ScheduleCell | undefined => {
      const lesson = lessonById.get(p.lesson_id);
      const block = blockById.get(p.time_block_id);
      if (!lesson || !block) return undefined;
      const classNames = lesson.school_classes.map((c) => c.name).join(", ");
      return {
        key: `${block.day_of_week}:${block.position}`,
        day: block.day_of_week,
        position: block.position,
        subjectName: lesson.subject.name,
        classNames,
        teacherName: lesson.teacher?.last_name ?? teacherById.get(p.teacher_id)?.last_name,
        roomName: "",
        lessonId: p.lesson_id,
        timeBlockId: p.time_block_id,
        roomId: p.room_id,
        pinned: p.pinned,
      };
    })
    .filter((c): c is ScheduleCell => c !== undefined);

  const daysPresent = Array.from(new Set(cells.map((c) => c.day))).sort((a, b) => a - b);
  const positions = Array.from(new Set(cells.map((c) => c.position))).sort((a, b) => a - b);

  return (
    <div className="space-y-5">
      <ScheduleToolbar
        view="room"
        rooms={rooms.data ?? []}
        roomId={roomId}
        onRoomChange={onRoomChange}
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
