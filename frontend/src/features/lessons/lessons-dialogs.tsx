import { zodResolver } from "@hookform/resolvers/zod";
import { useRef, useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useSchoolClasses } from "@/features/school-classes/hooks";
import { useSubjects } from "@/features/subjects/hooks";
import { useTeachers } from "@/features/teachers/hooks";
import { ApiError } from "@/lib/api-client";
import {
  type Lesson,
  type LessonCreate,
  type LessonUpdate,
  useCreateLesson,
  useDeleteLesson,
  useUpdateLesson,
} from "./hooks";
import { LessonFormSchema, type LessonFormValues } from "./schema";

interface LessonFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  submitLabel: string;
  lesson?: Lesson;
}

function joinLessonClassNames(lesson: Lesson): string {
  return lesson.school_classes.map((c) => c.name).join(", ");
}

export function LessonFormDialog({
  open,
  onOpenChange,
  submitLabel,
  lesson,
}: LessonFormDialogProps) {
  const { t } = useTranslation();
  const schoolClasses = useSchoolClasses();
  const subjects = useSubjects();
  const teachers = useTeachers();

  const initialTeacherId = lesson?.teacher?.id ?? null;
  const [pinIntent, setPinIntent] = useState<boolean>(initialTeacherId !== null);
  const lastPinnedRef = useRef<string | null>(initialTeacherId);

  const form = useForm<LessonFormValues>({
    resolver: zodResolver(LessonFormSchema),
    defaultValues: {
      school_class_ids: lesson?.school_classes.map((c) => c.id) ?? [],
      subject_id: lesson?.subject.id ?? "",
      teacher_id: lesson?.teacher?.id ?? null,
      hours_per_week: lesson?.hours_per_week ?? 1,
      preferred_block_size: lesson?.preferred_block_size ?? 1,
    },
  });
  const createMutation = useCreateLesson();
  const updateMutation = useUpdateLesson();
  const submitting = createMutation.isPending || updateMutation.isPending;

  const classOptions = schoolClasses.data ?? [];
  const subjectOptions = subjects.data ?? [];
  const teacherOptions = teachers.data ?? [];
  const missingPrereqs =
    !schoolClasses.isLoading &&
    !subjects.isLoading &&
    (classOptions.length === 0 || subjectOptions.length === 0);

  const subjectId = form.watch("subject_id");
  const currentTeacherId = form.watch("teacher_id");
  const qualifiedTeacherOptions = subjectId
    ? teacherOptions.filter((opt) => (opt.subject_ids ?? []).includes(subjectId))
    : teacherOptions;
  const currentTeacher = currentTeacherId
    ? (teacherOptions.find((opt) => opt.id === currentTeacherId) ?? null)
    : null;
  const isCurrentTeacherUnqualified =
    currentTeacher !== null && !qualifiedTeacherOptions.some((opt) => opt.id === currentTeacher.id);
  const optionsForRender = isCurrentTeacherUnqualified
    ? [...qualifiedTeacherOptions, currentTeacher]
    : qualifiedTeacherOptions;
  const firstQualifiedId = qualifiedTeacherOptions[0]?.id ?? null;
  const showEmptyQualified =
    pinIntent &&
    !teachers.isLoading &&
    qualifiedTeacherOptions.length === 0 &&
    currentTeacher === null;

  const title = lesson ? t("lessons.dialog.editTitle") : t("lessons.dialog.createTitle");
  const description = lesson
    ? t("lessons.dialog.editDescription", {
        className: joinLessonClassNames(lesson),
        subjectName: lesson.subject.name,
      })
    : t("lessons.dialog.createDescription");

  async function handleLessonSubmit(values: LessonFormValues) {
    const createBody: LessonCreate = {
      school_class_ids: values.school_class_ids,
      subject_id: values.subject_id,
      teacher_id: values.teacher_id,
      hours_per_week: values.hours_per_week,
      preferred_block_size: values.preferred_block_size,
    };
    const updateBody: LessonUpdate = {
      teacher_id: values.teacher_id,
      hours_per_week: values.hours_per_week,
      preferred_block_size: values.preferred_block_size,
    };
    try {
      if (lesson) {
        await updateMutation.mutateAsync({ id: lesson.id, body: updateBody });
      } else {
        await createMutation.mutateAsync(createBody);
      }
      form.reset();
      onOpenChange(false);
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        form.setError("root", { message: t("lessons.errors.duplicate") });
        return;
      }
      throw err;
    }
  }

  const rootError = form.formState.errors.root?.message;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) form.reset();
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        {missingPrereqs ? (
          <div
            role="status"
            aria-live="polite"
            className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm text-foreground"
          >
            <p>{t("lessons.dialog.missingPrereqs")}</p>
            <div className="mt-2 flex flex-wrap gap-3 text-sm font-medium">
              {classOptions.length === 0 ? (
                <a href="/school-classes" className="underline">
                  {t("lessons.dialog.addSchoolClass")}
                </a>
              ) : null}
              {subjectOptions.length === 0 ? (
                <a href="/subjects" className="underline">
                  {t("lessons.dialog.addSubject")}
                </a>
              ) : null}
            </div>
          </div>
        ) : null}
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(handleLessonSubmit)}>
            <FormField
              control={form.control}
              name="school_class_ids"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("lessons.form.classes")}</FormLabel>
                  <div className="max-h-48 space-y-1 overflow-y-auto rounded-md border p-2">
                    {classOptions.map((cls) => {
                      const checked = field.value.includes(cls.id);
                      const checkboxId = `lesson-class-${cls.id}`;
                      return (
                        <div key={cls.id} className="flex items-center gap-2">
                          <Checkbox
                            id={checkboxId}
                            checked={checked}
                            onCheckedChange={(next) => {
                              if (next === true && !checked) {
                                field.onChange([...field.value, cls.id]);
                              } else if (next !== true && checked) {
                                field.onChange(field.value.filter((id: string) => id !== cls.id));
                              }
                            }}
                            aria-label={cls.name}
                          />
                          <label htmlFor={checkboxId} className="cursor-pointer">
                            {cls.name}
                          </label>
                        </div>
                      );
                    })}
                  </div>
                  {form.formState.errors.school_class_ids ? (
                    <p role="alert" className="text-sm font-medium text-destructive">
                      {t("lessons.form.classesRequired")}
                    </p>
                  ) : null}
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="subject_id"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("lessons.fields.subjectLabel")}</FormLabel>
                  <Select value={field.value} onValueChange={field.onChange}>
                    <FormControl>
                      <SelectTrigger>
                        <SelectValue placeholder={t("lessons.fields.subjectPlaceholder")} />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      {subjectOptions.map((option) => (
                        <SelectItem key={option.id} value={option.id}>
                          {option.name} · {option.short_name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="teacher_id"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("lessons.fields.teacherLabel")}</FormLabel>
                  <div className="flex items-center gap-2">
                    <Switch
                      id="lesson-pin-teacher"
                      checked={pinIntent}
                      aria-label={t("lessons.fields.pinTeacherLabel")}
                      onCheckedChange={(next) => {
                        if (next === false) {
                          setPinIntent(false);
                          form.setValue("teacher_id", null);
                          return;
                        }
                        setPinIntent(true);
                        const restored = lastPinnedRef.current ?? firstQualifiedId ?? null;
                        form.setValue("teacher_id", restored);
                      }}
                    />
                    <label htmlFor="lesson-pin-teacher" className="cursor-pointer text-sm">
                      {t("lessons.fields.pinTeacherLabel")}
                    </label>
                  </div>
                  {pinIntent ? (
                    showEmptyQualified ? (
                      <p className="text-sm text-muted-foreground">
                        {t("lessons.fields.teacherEmptyQualified")}
                      </p>
                    ) : (
                      <Select
                        value={field.value ?? ""}
                        onValueChange={(next) => {
                          field.onChange(next);
                          lastPinnedRef.current = next;
                        }}
                      >
                        <FormControl>
                          <SelectTrigger>
                            <SelectValue placeholder={t("lessons.fields.teacherPlaceholder")} />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          {optionsForRender.map((option) => {
                            const isUnqualified =
                              isCurrentTeacherUnqualified &&
                              currentTeacher !== null &&
                              option.id === currentTeacher.id;
                            return (
                              <SelectItem key={option.id} value={option.id}>
                                {option.first_name} {option.last_name} ({option.short_code})
                                {isUnqualified
                                  ? ` ${t("lessons.fields.teacherNotQualifiedSuffix")}`
                                  : ""}
                              </SelectItem>
                            );
                          })}
                        </SelectContent>
                      </Select>
                    )
                  ) : (
                    <p className="text-sm text-muted-foreground">
                      {t("lessons.fields.pinTeacherHint")}
                    </p>
                  )}
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="hours_per_week"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("lessons.fields.hoursPerWeekLabel")}</FormLabel>
                  <FormControl>
                    <Input
                      type="number"
                      min={1}
                      max={40}
                      value={field.value}
                      onChange={(e) =>
                        field.onChange(e.target.value === "" ? 0 : Number(e.target.value))
                      }
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="preferred_block_size"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("lessons.fields.blockSizeLabel")}</FormLabel>
                  <Select
                    value={String(field.value)}
                    onValueChange={(value) => field.onChange(Number(value))}
                  >
                    <FormControl>
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      <SelectItem value="1">{t("lessons.fields.blockSizeSingle")}</SelectItem>
                      <SelectItem value="2">{t("lessons.fields.blockSizeDouble")}</SelectItem>
                    </SelectContent>
                  </Select>
                  <p className="text-xs text-muted-foreground">
                    {t("lessons.fields.blockSizeHelp")}
                  </p>
                  <FormMessage />
                </FormItem>
              )}
            />
            {rootError ? (
              <p role="alert" className="text-sm font-medium text-destructive">
                {rootError}
              </p>
            ) : null}
            <DialogFooter>
              <Button type="submit" disabled={submitting || missingPrereqs || showEmptyQualified}>
                {submitting ? t("common.saving") : submitLabel}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}

interface DeleteLessonDialogProps {
  lesson: Lesson;
  onClose: () => void;
}

export function DeleteLessonDialog({ lesson, onClose }: DeleteLessonDialogProps) {
  const { t } = useTranslation();
  const mutation = useDeleteLesson();
  return (
    <ConfirmDialog
      open
      onClose={onClose}
      title={t("lessons.dialog.deleteTitle")}
      description={t("lessons.dialog.deleteDescription", {
        className: joinLessonClassNames(lesson),
        subjectName: lesson.subject.name,
      })}
      isPending={mutation.isPending}
      onConfirm={async () => {
        await mutation.mutateAsync(lesson.id);
        onClose();
      }}
    />
  );
}
