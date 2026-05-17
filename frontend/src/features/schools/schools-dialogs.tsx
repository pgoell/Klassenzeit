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
import { type School, useCreateSchool, useDeleteSchool, useUpdateSchool } from "./hooks";
import { SchoolFormSchema, type SchoolFormValues } from "./schema";

interface SchoolFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  submitLabel: string;
  school?: School;
}

export function SchoolFormDialog({
  open,
  onOpenChange,
  submitLabel,
  school,
}: SchoolFormDialogProps) {
  const { t } = useTranslation();
  const form = useForm<SchoolFormValues>({
    resolver: zodResolver(SchoolFormSchema),
    defaultValues: {
      name: school?.name ?? "",
      short_name: school?.short_name ?? "",
    },
    values: school ? { name: school.name, short_name: school.short_name ?? "" } : undefined,
  });
  const createMutation = useCreateSchool();
  const updateMutation = useUpdateSchool();
  const submitting = createMutation.isPending || updateMutation.isPending;

  const title = school ? t("schools.dialog.editTitle") : t("schools.dialog.createTitle");
  const description = school
    ? t("schools.dialog.editDescription", { name: school.name })
    : t("schools.dialog.createDescription");

  async function handleSubmit(values: SchoolFormValues) {
    const short_name = values.short_name && values.short_name.length > 0 ? values.short_name : null;
    if (school) {
      await updateMutation.mutateAsync({
        id: school.id,
        body: { name: values.name, short_name },
      });
    } else {
      await createMutation.mutateAsync({ name: values.name, short_name });
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
          <form className="space-y-4" onSubmit={form.handleSubmit(handleSubmit)}>
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("schools.columns.name")}</FormLabel>
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
                  <FormLabel>{t("schools.columns.shortName")}</FormLabel>
                  <FormControl>
                    <Input {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <DialogFooter>
              <Button type="submit" disabled={submitting}>
                {submitLabel}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}

interface DeleteSchoolDialogProps {
  school: School;
  onClose: () => void;
}

export function DeleteSchoolDialog({ school, onClose }: DeleteSchoolDialogProps) {
  const { t } = useTranslation();
  const deleteMutation = useDeleteSchool();
  return (
    <ConfirmDialog
      open
      onClose={onClose}
      title={t("schools.dialog.deleteTitle")}
      description={t("schools.dialog.deleteDescription", { name: school.name })}
      confirmLabel={t("common.delete")}
      isPending={deleteMutation.isPending}
      onConfirm={async () => {
        await deleteMutation.mutateAsync(school.id);
        onClose();
      }}
    />
  );
}
