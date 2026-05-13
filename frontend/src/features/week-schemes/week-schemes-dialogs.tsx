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
import { Textarea } from "@/components/ui/textarea";
import {
  useCreateWeekScheme,
  useDeleteWeekScheme,
  useUpdateWeekScheme,
  type WeekScheme,
} from "./hooks";
import { WeekSchemeFormSchema, type WeekSchemeFormValues } from "./schema";
import { TimeBlocksGrid } from "./time-blocks-grid";

interface WeekSchemeFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  submitLabel: string;
  scheme?: WeekScheme;
}

export function WeekSchemeFormDialog({
  open,
  onOpenChange,
  submitLabel,
  scheme,
}: WeekSchemeFormDialogProps) {
  const { t } = useTranslation();
  const form = useForm<WeekSchemeFormValues>({
    resolver: zodResolver(WeekSchemeFormSchema),
    defaultValues: {
      name: scheme?.name ?? "",
      description: scheme?.description ?? "",
    },
  });
  const createMutation = useCreateWeekScheme();
  const updateMutation = useUpdateWeekScheme();
  const submitting = createMutation.isPending || updateMutation.isPending;

  const title = scheme ? t("weekSchemes.dialog.editTitle") : t("weekSchemes.dialog.createTitle");
  const description = scheme
    ? t("weekSchemes.dialog.editDescription", { name: scheme.name })
    : t("weekSchemes.dialog.createDescription");

  async function handleWeekSchemeSubmit(values: WeekSchemeFormValues) {
    const body = {
      name: values.name,
      description: values.description ? values.description : null,
    };
    if (scheme) {
      await updateMutation.mutateAsync({ id: scheme.id, body });
    } else {
      await createMutation.mutateAsync(body);
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
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(handleWeekSchemeSubmit)}>
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("weekSchemes.columns.name")}</FormLabel>
                  <FormControl>
                    <Input autoFocus {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="description"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("weekSchemes.columns.description")}</FormLabel>
                  <FormControl>
                    <Textarea rows={3} {...field} value={field.value ?? ""} />
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
        {scheme ? (
          <div className="border-t pt-4">
            <TimeBlocksGrid schemeId={scheme.id} />
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

interface DeleteWeekSchemeDialogProps {
  scheme: WeekScheme;
  onClose: () => void;
}

export function DeleteWeekSchemeDialog({ scheme, onClose }: DeleteWeekSchemeDialogProps) {
  const { t } = useTranslation();
  const mutation = useDeleteWeekScheme();
  return (
    <ConfirmDialog
      open
      onClose={onClose}
      title={t("weekSchemes.dialog.deleteTitle")}
      description={t("weekSchemes.dialog.deleteDescription", { name: scheme.name })}
      isPending={mutation.isPending}
      onConfirm={async () => {
        await mutation.mutateAsync(scheme.id);
        onClose();
      }}
    />
  );
}
