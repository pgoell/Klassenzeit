import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { Button } from "@/components/ui/button";
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
import { useSchoolClasses } from "@/features/school-classes/hooks";
import { type Teacher, useCreateTeacher, useDeleteTeacher, useUpdateTeacher } from "./hooks";
import { TeacherFormSchema, type TeacherFormValues } from "./schema";
import { TeacherAvailabilityGrid } from "./teacher-availability-grid";
import { TeacherQualificationsEditor } from "./teacher-qualifications-editor";
import { WeekdayPicker } from "./weekday-picker";

interface TeacherFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  submitLabel: string;
  teacher?: Teacher;
}

export function TeacherFormDialog({
  open,
  onOpenChange,
  submitLabel,
  teacher,
}: TeacherFormDialogProps) {
  const { t } = useTranslation();
  const form = useForm<TeacherFormValues>({
    resolver: zodResolver(TeacherFormSchema),
    defaultValues: {
      first_name: teacher?.first_name ?? "",
      last_name: teacher?.last_name ?? "",
      short_code: teacher?.short_code ?? "",
      max_hours_per_week: teacher?.max_hours_per_week ?? 1,
      reserve_hours_per_week: teacher?.reserve_hours_per_week ?? 0,
      working_days: teacher?.working_days ?? null,
    },
  });
  const createMutation = useCreateTeacher();
  const updateMutation = useUpdateTeacher();
  const submitting = createMutation.isPending || updateMutation.isPending;

  const title = teacher ? t("teachers.dialog.editTitle") : t("teachers.dialog.createTitle");
  const description = teacher
    ? t("teachers.dialog.editDescription", {
        name: `${teacher.first_name} ${teacher.last_name}`,
      })
    : t("teachers.dialog.createDescription");

  const schoolClasses = useSchoolClasses();
  const klassenlehrerOf = teacher
    ? (schoolClasses.data ?? []).filter((c) => c.class_teacher_id === teacher.id)
    : [];

  async function handleTeacherSubmit(values: TeacherFormValues) {
    if (teacher) {
      await updateMutation.mutateAsync({ id: teacher.id, body: values });
    } else {
      await createMutation.mutateAsync(values);
    }
    form.reset();
    onOpenChange(false);
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) form.reset();
        onOpenChange(next);
      }}
    >
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
          {klassenlehrerOf.length > 0 ? (
            <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
              <span className="font-medium">{t("teachers.dialog.klassenlehrerOfLabel")}:</span>
              <span>{klassenlehrerOf.map((c) => c.name).join(", ")}</span>
            </div>
          ) : null}
        </DialogHeader>
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(handleTeacherSubmit)}>
            <FormField
              control={form.control}
              name="first_name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("teachers.columns.firstName")}</FormLabel>
                  <FormControl>
                    <Input autoFocus {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="last_name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("teachers.columns.lastName")}</FormLabel>
                  <FormControl>
                    <Input {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="short_code"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("teachers.columns.shortCode")}</FormLabel>
                  <FormControl>
                    <Input {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="max_hours_per_week"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("teachers.columns.maxHoursPerWeek")}</FormLabel>
                  <FormControl>
                    <Input
                      type="number"
                      min={1}
                      value={field.value}
                      onChange={(e) => field.onChange(Number(e.target.value))}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="reserve_hours_per_week"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("teachers.fields.reserveHoursPerWeek")}</FormLabel>
                  <FormControl>
                    <Input
                      type="number"
                      min={0}
                      value={field.value}
                      onChange={(e) => field.onChange(Number(e.target.value))}
                    />
                  </FormControl>
                  <FormMessage />
                  {form.watch("max_hours_per_week") > 0 &&
                    form.watch("reserve_hours_per_week") >= form.watch("max_hours_per_week") && (
                      <p className="text-sm text-amber-600">
                        {t("teachers.fields.reserveExceedsMaxWarning")}
                      </p>
                    )}
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="working_days"
              render={({ field }) => (
                <FormItem>
                  <FormControl>
                    <WeekdayPicker value={field.value} onChange={field.onChange} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <DialogFooter>
              <Button type="submit" disabled={submitting}>
                {submitting ? t("common.saving") : submitLabel}
              </Button>
            </DialogFooter>
          </form>
        </Form>
        {teacher ? (
          <>
            <TeacherQualificationsEditor teacherId={teacher.id} />
            <TeacherAvailabilityGrid teacherId={teacher.id} />
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

interface DeleteTeacherDialogProps {
  teacher: Teacher;
  onClose: () => void;
}

export function DeleteTeacherDialog({ teacher, onClose }: DeleteTeacherDialogProps) {
  const { t } = useTranslation();
  const mutation = useDeleteTeacher();
  return (
    <ConfirmDialog
      open
      onClose={onClose}
      title={t("teachers.dialog.deleteTitle")}
      description={t("teachers.dialog.deleteDescription", {
        name: `${teacher.first_name} ${teacher.last_name}`,
      })}
      isPending={mutation.isPending}
      onConfirm={async () => {
        await mutation.mutateAsync(teacher.id);
        onClose();
      }}
    />
  );
}
