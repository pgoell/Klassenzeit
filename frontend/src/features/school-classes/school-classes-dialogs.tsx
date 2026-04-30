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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useRooms } from "@/features/rooms/hooks";
import { useStundentafeln } from "@/features/stundentafeln/hooks";
import { useWeekSchemes } from "@/features/week-schemes/hooks";
import {
  type SchoolClass,
  useCreateSchoolClass,
  useDeleteSchoolClass,
  useUpdateSchoolClass,
} from "./hooks";
import { SchoolClassFormSchema, type SchoolClassFormValues } from "./schema";

interface SchoolClassFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  submitLabel: string;
  schoolClass?: SchoolClass;
}

export function SchoolClassFormDialog({
  open,
  onOpenChange,
  submitLabel,
  schoolClass,
}: SchoolClassFormDialogProps) {
  const { t } = useTranslation();
  const stundentafeln = useStundentafeln();
  const weekSchemes = useWeekSchemes();
  const rooms = useRooms();

  const form = useForm<SchoolClassFormValues>({
    resolver: zodResolver(SchoolClassFormSchema),
    defaultValues: {
      name: schoolClass?.name ?? "",
      grade_level: schoolClass?.grade_level ?? 1,
      stundentafel_id: schoolClass?.stundentafel_id ?? "",
      week_scheme_id: schoolClass?.week_scheme_id ?? "",
      home_room_id: schoolClass?.home_room_id ?? null,
    },
  });
  const createMutation = useCreateSchoolClass();
  const updateMutation = useUpdateSchoolClass();
  const submitting = createMutation.isPending || updateMutation.isPending;

  const stundentafelOptions = stundentafeln.data ?? [];
  const weekSchemeOptions = weekSchemes.data ?? [];
  const roomOptions = rooms.data ?? [];
  const missingPrereqs =
    !stundentafeln.isLoading &&
    !weekSchemes.isLoading &&
    (stundentafelOptions.length === 0 || weekSchemeOptions.length === 0);

  const title = schoolClass
    ? t("schoolClasses.dialog.editTitle")
    : t("schoolClasses.dialog.createTitle");
  const description = schoolClass
    ? t("schoolClasses.dialog.editDescription", { name: schoolClass.name })
    : t("schoolClasses.dialog.createDescription");

  async function handleSchoolClassSubmit(values: SchoolClassFormValues) {
    const body = {
      name: values.name,
      grade_level: values.grade_level,
      stundentafel_id: values.stundentafel_id,
      week_scheme_id: values.week_scheme_id,
      home_room_id: values.home_room_id,
    };
    if (schoolClass) {
      await updateMutation.mutateAsync({ id: schoolClass.id, body });
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
            <p>{t("schoolClasses.dialog.missingPrereqs")}</p>
            <div className="mt-2 flex flex-wrap gap-3 text-sm font-medium">
              {stundentafelOptions.length === 0 ? (
                <a href="/stundentafeln" className="underline">
                  {t("schoolClasses.dialog.addStundentafel")}
                </a>
              ) : null}
              {weekSchemeOptions.length === 0 ? (
                <a href="/week-schemes" className="underline">
                  {t("schoolClasses.dialog.addWeekScheme")}
                </a>
              ) : null}
            </div>
          </div>
        ) : null}
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(handleSchoolClassSubmit)}>
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("schoolClasses.columns.name")}</FormLabel>
                  <FormControl>
                    <Input autoFocus {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="grade_level"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("schoolClasses.fields.gradeLevelLabel")}</FormLabel>
                  <FormControl>
                    <Input
                      type="number"
                      min={1}
                      max={13}
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
              name="stundentafel_id"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("schoolClasses.fields.stundentafelLabel")}</FormLabel>
                  <Select value={field.value} onValueChange={field.onChange}>
                    <FormControl>
                      <SelectTrigger>
                        <SelectValue
                          placeholder={t("schoolClasses.fields.stundentafelPlaceholder")}
                        />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      {stundentafelOptions.map((option) => (
                        <SelectItem key={option.id} value={option.id}>
                          {option.name}
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
              name="week_scheme_id"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("schoolClasses.fields.weekSchemeLabel")}</FormLabel>
                  <Select value={field.value} onValueChange={field.onChange}>
                    <FormControl>
                      <SelectTrigger>
                        <SelectValue
                          placeholder={t("schoolClasses.fields.weekSchemePlaceholder")}
                        />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      {weekSchemeOptions.map((option) => (
                        <SelectItem key={option.id} value={option.id}>
                          {option.name}
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
              name="home_room_id"
              render={({ field }) => {
                const NULL_VALUE = "__none__";
                const value = field.value ?? NULL_VALUE;
                return (
                  <FormItem>
                    <FormLabel>{t("schoolClasses.fields.homeRoomLabel")}</FormLabel>
                    <Select
                      value={value}
                      onValueChange={(next) => field.onChange(next === NULL_VALUE ? null : next)}
                    >
                      <FormControl>
                        <SelectTrigger>
                          <SelectValue
                            placeholder={t("schoolClasses.fields.homeRoomPlaceholder")}
                          />
                        </SelectTrigger>
                      </FormControl>
                      <SelectContent>
                        <SelectItem value={NULL_VALUE}>
                          {t("schoolClasses.fields.homeRoomPlaceholder")}
                        </SelectItem>
                        {roomOptions.map((option) => (
                          <SelectItem key={option.id} value={option.id}>
                            {option.name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <FormMessage />
                  </FormItem>
                );
              }}
            />
            <DialogFooter>
              <Button type="submit" disabled={submitting || missingPrereqs}>
                {submitting ? t("common.saving") : submitLabel}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}

interface DeleteSchoolClassDialogProps {
  schoolClass: SchoolClass;
  onClose: () => void;
}

export function DeleteSchoolClassDialog({ schoolClass, onClose }: DeleteSchoolClassDialogProps) {
  const { t } = useTranslation();
  const mutation = useDeleteSchoolClass();
  return (
    <ConfirmDialog
      open
      onClose={onClose}
      title={t("schoolClasses.dialog.deleteTitle")}
      description={t("schoolClasses.dialog.deleteDescription", { name: schoolClass.name })}
      isPending={mutation.isPending}
      onConfirm={async () => {
        await mutation.mutateAsync(schoolClass.id);
        onClose();
      }}
    />
  );
}
