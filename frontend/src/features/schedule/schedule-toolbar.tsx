import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { Room } from "@/features/rooms/hooks";
import type { SchoolClass } from "@/features/school-classes/hooks";
import type { Teacher } from "@/features/teachers/hooks";
import { ApiError } from "@/lib/api-client";
import { useGenerateAllSchedules } from "./hooks";

type ClassToolbarProps = {
  view: "class";
  classes: SchoolClass[];
  classId: string | undefined;
  onClassChange: (id: string) => void;
  onGenerate: () => void;
  onCancelConfirm: () => void;
  placementsCount: number;
  confirming: boolean;
  pending: boolean;
};

type TeacherToolbarProps = {
  view: "teacher";
  teachers: Teacher[];
  teacherId: string | undefined;
  onTeacherChange: (id: string) => void;
};

type RoomToolbarProps = {
  view: "room";
  rooms: Room[];
  roomId: string | undefined;
  onRoomChange: (id: string) => void;
};

type ScheduleToolbarProps = ClassToolbarProps | TeacherToolbarProps | RoomToolbarProps;

type PickerLabelKey =
  | "schedule.picker.class.label"
  | "schedule.picker.teacher.label"
  | "schedule.picker.room.label";

type PickerPlaceholderKey =
  | "schedule.picker.class.placeholder"
  | "schedule.picker.teacher.placeholder"
  | "schedule.picker.room.placeholder";

export function ScheduleToolbar(props: ScheduleToolbarProps) {
  const { t } = useTranslation();
  const generateAll = useGenerateAllSchedules();

  const runGenerateAll = async () => {
    try {
      const result = await generateAll.mutateAsync();
      toast.success(
        t("schedule.generate.allSuccessToast", {
          classes: result.classes.length,
          placements: result.total_placements,
          violations: result.total_violations,
        }),
      );
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : t("schedule.generate.allErrorToast");
      toast.error(msg || t("schedule.generate.allErrorToast"));
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-end justify-between gap-3 rounded-lg border bg-card px-4 py-3">
        <div className="min-w-[220px] space-y-1">
          {props.view === "class" ? (
            <PickerSlot
              labelKey="schedule.picker.class.label"
              placeholderKey="schedule.picker.class.placeholder"
              triggerId="schedule-class-picker"
              value={props.classId}
              onValueChange={props.onClassChange}
              items={props.classes.map((c) => ({ id: c.id, label: c.name }))}
            />
          ) : null}
          {props.view === "teacher" ? (
            <PickerSlot
              labelKey="schedule.picker.teacher.label"
              placeholderKey="schedule.picker.teacher.placeholder"
              triggerId="schedule-teacher-picker"
              value={props.teacherId}
              onValueChange={props.onTeacherChange}
              items={props.teachers.map((tt) => ({
                id: tt.id,
                label: `${tt.first_name} ${tt.last_name}`,
              }))}
            />
          ) : null}
          {props.view === "room" ? (
            <PickerSlot
              labelKey="schedule.picker.room.label"
              placeholderKey="schedule.picker.room.placeholder"
              triggerId="schedule-room-picker"
              value={props.roomId}
              onValueChange={props.onRoomChange}
              items={props.rooms.map((r) => ({ id: r.id, label: r.name }))}
            />
          ) : null}
        </div>
        <div className="flex gap-2">
          {props.view === "class" ? (
            <Button
              onClick={props.onGenerate}
              variant="secondary"
              disabled={props.pending || !props.classId}
            >
              {props.pending ? t("common.saving") : t("schedule.generate.action")}
            </Button>
          ) : null}
          <Button variant="default" onClick={runGenerateAll} disabled={generateAll.isPending}>
            {t("schedule.generate.allAction")}
          </Button>
        </div>
      </div>
      {props.view === "class" && props.confirming ? (
        <div
          role="alert"
          aria-live="polite"
          className="flex flex-wrap items-center gap-3 rounded-md border border-amber-400/60 bg-amber-50 px-3 py-2 text-sm text-amber-900 dark:border-amber-400/40 dark:bg-amber-950/40 dark:text-amber-200"
        >
          <span>{t("schedule.generate.replaceBanner", { count: props.placementsCount })}</span>
          <div className="ml-auto flex gap-2">
            <Button size="sm" variant="outline" onClick={props.onCancelConfirm}>
              {t("schedule.generate.cancel")}
            </Button>
            <Button size="sm" onClick={props.onGenerate} disabled={props.pending}>
              {t("schedule.generate.confirmReplace")}
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  );
}

interface PickerSlotProps {
  labelKey: PickerLabelKey;
  placeholderKey: PickerPlaceholderKey;
  triggerId: string;
  value: string | undefined;
  onValueChange: (id: string) => void;
  items: Array<{ id: string; label: string }>;
}

function PickerSlot({
  labelKey,
  placeholderKey,
  triggerId,
  value,
  onValueChange,
  items,
}: PickerSlotProps) {
  const { t } = useTranslation();
  return (
    <>
      <label htmlFor={triggerId} className="block text-xs font-medium text-muted-foreground">
        {t(labelKey)}
      </label>
      <Select value={value ?? ""} onValueChange={onValueChange}>
        <SelectTrigger id={triggerId} aria-label={t(labelKey)}>
          <SelectValue placeholder={t(placeholderKey)} />
        </SelectTrigger>
        <SelectContent>
          {items.map((it) => (
            <SelectItem key={it.id} value={it.id}>
              {it.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </>
  );
}
