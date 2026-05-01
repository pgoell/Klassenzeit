import { zodResolver } from "@hookform/resolvers/zod";
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
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import { autoPickColor } from "./color";
import { ColorPicker } from "./color-picker";
import { type Subject, useCreateSubject, useDeleteSubject, useUpdateSubject } from "./hooks";
import { SubjectFormSchema, type SubjectFormValues } from "./schema";

interface SubjectFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  submitLabel: string;
  subject?: Subject;
}

export function SubjectFormDialog({
  open,
  onOpenChange,
  submitLabel,
  subject,
}: SubjectFormDialogProps) {
  const { t } = useTranslation();
  const form = useForm<SubjectFormValues>({
    resolver: zodResolver(SubjectFormSchema),
    defaultValues: {
      name: subject?.name ?? "",
      short_name: subject?.short_name ?? "",
      color: subject?.color ?? autoPickColor(""),
      prefer_early_periods: subject?.prefer_early_periods ?? false,
      avoid_first_period: subject?.avoid_first_period ?? false,
      avoid_last_period: subject?.avoid_last_period ?? false,
    },
  });
  const createMutation = useCreateSubject();
  const updateMutation = useUpdateSubject();
  const submitting = createMutation.isPending || updateMutation.isPending;

  const title = subject ? t("subjects.dialog.editTitle") : t("subjects.dialog.createTitle");
  const description = subject
    ? t("subjects.dialog.editDescription", { name: subject.name })
    : t("subjects.dialog.createDescription");

  async function handleSubjectSubmit(values: SubjectFormValues) {
    if (subject) {
      await updateMutation.mutateAsync({ id: subject.id, body: values });
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
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(handleSubjectSubmit)}>
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("subjects.columns.name")}</FormLabel>
                  <FormControl>
                    <Input autoFocus {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="short_name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("subjects.columns.shortName")}</FormLabel>
                  <FormControl>
                    <Input {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="color"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("subjects.color")}</FormLabel>
                  <FormControl>
                    <ColorPicker value={field.value} onChange={field.onChange} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="prefer_early_periods"
              render={({ field }) => (
                <FormItem className="flex flex-col gap-1">
                  <div className="flex items-center gap-2">
                    <FormControl>
                      <Checkbox
                        id="subject-prefer-early"
                        checked={field.value}
                        onCheckedChange={field.onChange}
                      />
                    </FormControl>
                    <FormLabel htmlFor="subject-prefer-early">
                      {t("subjects.fields.preferEarlyPeriods.label")}
                    </FormLabel>
                  </div>
                  <FormDescription>{t("subjects.fields.preferEarlyPeriods.help")}</FormDescription>
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="avoid_first_period"
              render={({ field }) => (
                <FormItem className="flex flex-col gap-1">
                  <div className="flex items-center gap-2">
                    <FormControl>
                      <Checkbox
                        id="subject-avoid-first"
                        checked={field.value}
                        onCheckedChange={field.onChange}
                      />
                    </FormControl>
                    <FormLabel htmlFor="subject-avoid-first">
                      {t("subjects.fields.avoidFirstPeriod.label")}
                    </FormLabel>
                  </div>
                  <FormDescription>{t("subjects.fields.avoidFirstPeriod.help")}</FormDescription>
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="avoid_last_period"
              render={({ field }) => (
                <FormItem className="flex flex-col gap-1">
                  <div className="flex items-center gap-2">
                    <FormControl>
                      <Checkbox
                        id="subject-avoid-last"
                        checked={field.value}
                        onCheckedChange={field.onChange}
                      />
                    </FormControl>
                    <FormLabel htmlFor="subject-avoid-last">
                      {t("subjects.fields.avoidLastPeriod.label")}
                    </FormLabel>
                  </div>
                  <FormDescription>{t("subjects.fields.avoidLastPeriod.help")}</FormDescription>
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
      </DialogContent>
    </Dialog>
  );
}

interface DeleteSubjectDialogProps {
  subject: Subject;
  onClose: () => void;
}

export function DeleteSubjectDialog({ subject, onClose }: DeleteSubjectDialogProps) {
  const { t } = useTranslation();
  const mutation = useDeleteSubject();
  return (
    <ConfirmDialog
      open
      onClose={onClose}
      title={t("subjects.dialog.deleteTitle")}
      description={t("subjects.dialog.deleteDescription", { name: subject.name })}
      isPending={mutation.isPending}
      onConfirm={async () => {
        await mutation.mutateAsync(subject.id);
        onClose();
      }}
    />
  );
}
