import { useNavigate, useSearch } from "@tanstack/react-router";
import { Calendar } from "lucide-react";
import { Fragment, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { EmptyState } from "@/components/empty-state";
import { Button } from "@/components/ui/button";
import { useLessons } from "@/features/lessons/hooks";
import { useRooms } from "@/features/rooms/hooks";
import { useSchoolClasses } from "@/features/school-classes/hooks";
import { useWeekSchemeDetail } from "@/features/week-schemes/hooks";
import { ApiError } from "@/lib/api-client";
import { useClassSchedule, useGenerateClassSchedule, type Violation } from "./hooks";
import { type ScheduleCell, ScheduleGrid } from "./schedule-grid";
import { ScheduleStatus } from "./schedule-status";
import { ScheduleToolbar } from "./schedule-toolbar";

const SKEL_DAYS = ["d1", "d2", "d3", "d4", "d5"] as const;
const SKEL_POSITIONS = ["p1", "p2", "p3", "p4", "p5", "p6"] as const;

export function SchedulePageClassView() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const search = useSearch({ strict: false }) as { class?: string };
  const classId = search.class;

  const classes = useSchoolClasses();
  const schedule = useClassSchedule(classId);
  const lessons = useLessons();
  const rooms = useRooms();
  const schoolClass = classes.data?.find((c) => c.id === classId);
  const weekScheme = useWeekSchemeDetail(schoolClass?.week_scheme_id ?? null);
  const generate = useGenerateClassSchedule();

  const [confirming, setConfirming] = useState(false);
  const [postViolations, setPostViolations] = useState<Violation[] | undefined>();

  function onClassChange(id: string) {
    setConfirming(false);
    setPostViolations(undefined);
    void navigate({ to: "/schedule", search: { class: id } });
  }

  const runScheduleGenerate = async () => {
    if (!classId) return;
    const placementsNow = schedule.data?.placements.length ?? 0;
    if (placementsNow > 0 && !confirming) {
      setConfirming(true);
      return;
    }
    try {
      const result = await generate.mutateAsync(classId);
      setPostViolations(result.violations);
      setConfirming(false);
      toast.success(t("schedule.generate.successToast", { count: result.placements.length }));
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : t("schedule.generate.errorToast");
      toast.error(msg || t("schedule.generate.errorToast"));
    }
  };

  if (!classId) {
    return (
      <div className="space-y-4">
        <ScheduleToolbar
          view="class"
          classes={classes.data ?? []}
          classId={undefined}
          onClassChange={onClassChange}
          onGenerate={runScheduleGenerate}
          onCancelConfirm={() => setConfirming(false)}
          placementsCount={0}
          confirming={false}
          pending={false}
        />
        <p className="text-sm text-muted-foreground">{t("schedule.picker.class.none")}</p>
      </div>
    );
  }

  const loading =
    classes.isLoading ||
    schedule.isLoading ||
    lessons.isLoading ||
    rooms.isLoading ||
    weekScheme.isLoading;
  const errored =
    classes.isError || schedule.isError || lessons.isError || rooms.isError || weekScheme.isError;

  const placements = schedule.data?.placements ?? [];
  const lessonById = new Map((lessons.data ?? []).map((l) => [l.id, l]));
  const roomById = new Map((rooms.data ?? []).map((r) => [r.id, r]));
  const timeBlockById = new Map((weekScheme.data?.time_blocks ?? []).map((b) => [b.id, b]));

  const classLessons = (lessons.data ?? []).filter((l) =>
    l.school_classes.some((c) => c.id === classId),
  );
  const expectedHours = classLessons.reduce((sum, l) => sum + l.hours_per_week, 0);

  const cells: ScheduleCell[] = placements
    .map((p): ScheduleCell | undefined => {
      const lesson = lessonById.get(p.lesson_id);
      const block = timeBlockById.get(p.time_block_id);
      if (!lesson || !block) return undefined;
      const room = roomById.get(p.room_id);
      return {
        key: `${block.day_of_week}:${block.position}`,
        day: block.day_of_week,
        position: block.position,
        subjectName: lesson.subject.name,
        teacherName: lesson.teacher?.last_name,
        roomName: room?.name ?? t("schedule.cellDeletedLesson"),
        lessonId: p.lesson_id,
        timeBlockId: p.time_block_id,
        pinned: p.pinned,
      };
    })
    .filter((c): c is ScheduleCell => c !== undefined);

  const daysPresent = Array.from(
    new Set((weekScheme.data?.time_blocks ?? []).map((b) => b.day_of_week)),
  ).sort((a, b) => a - b);
  const positions = Array.from(
    new Set((weekScheme.data?.time_blocks ?? []).map((b) => b.position)),
  ).sort((a, b) => a - b);

  return (
    <>
      <ScheduleToolbar
        view="class"
        classes={classes.data ?? []}
        classId={classId}
        onClassChange={onClassChange}
        onGenerate={runScheduleGenerate}
        onCancelConfirm={() => setConfirming(false)}
        placementsCount={placements.length}
        confirming={confirming}
        pending={generate.isPending}
      />
      {loading ? (
        <ScheduleSkeletonGrid />
      ) : errored ? (
        <div className="space-y-2 text-sm text-destructive">
          <p>{t("schedule.loadError")}</p>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              void schedule.refetch();
            }}
          >
            {t("schedule.retry")}
          </Button>
        </div>
      ) : placements.length === 0 ? (
        <EmptyState
          icon={<Calendar className="h-7 w-7" />}
          title={t("schedule.empty.title")}
          body={t("schedule.empty.body")}
          steps={[t("schedule.empty.step1"), t("schedule.empty.step2"), t("schedule.empty.step3")]}
          onCreate={() => {
            void runScheduleGenerate();
          }}
          createLabel={t("schedule.generate.action")}
        />
      ) : (
        <>
          <ScheduleStatus
            placementsCount={placements.length}
            expectedHours={expectedHours}
            violations={postViolations}
            lessonById={lessonById}
          />
          <ScheduleGrid cells={cells} daysPresent={daysPresent} positions={positions} />
        </>
      )}
    </>
  );
}

function ScheduleSkeletonGrid() {
  return (
    <div
      className="kz-ws-grid animate-pulse"
      style={{ gridTemplateColumns: `56px repeat(${SKEL_DAYS.length}, 1fr)` }}
    >
      <div className="kz-ws-cell" data-variant="header" />
      {SKEL_DAYS.map((d) => (
        <div key={`skel-h-${d}`} className="kz-ws-cell" data-variant="header" />
      ))}
      {SKEL_POSITIONS.map((p) => (
        <Fragment key={`skel-row-${p}`}>
          <div className="kz-ws-cell" data-variant="time" />
          {SKEL_DAYS.map((d) => (
            <div key={`skel-${d}-${p}`} className="kz-ws-cell" />
          ))}
        </Fragment>
      ))}
    </div>
  );
}
