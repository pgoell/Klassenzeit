import { zodResolver } from "@hookform/resolvers/zod";
import { useState } from "react";
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useSubjects } from "@/features/subjects/hooks";
import { ApiError } from "@/lib/api-client";
import {
  type EntryCreate,
  type EntryUpdate,
  type Stundentafel,
  type StundentafelCreate,
  type StundentafelEntry,
  type StundentafelUpdate,
  useCreateStundentafel,
  useCreateStundentafelEntry,
  useDeleteStundentafel,
  useDeleteStundentafelEntry,
  useStundentafel,
  useUpdateStundentafel,
  useUpdateStundentafelEntry,
} from "./hooks";
import {
  EntryFormSchema,
  type EntryFormValues,
  SchoolTypeValues,
  StundentafelFormSchema,
  type StundentafelFormValues,
} from "./schema";
import { schoolTypeLabelKey } from "./school-type-keys";

interface StundentafelFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function StundentafelFormDialog({ open, onOpenChange }: StundentafelFormDialogProps) {
  const { t } = useTranslation();
  const form = useForm<StundentafelFormValues>({
    resolver: zodResolver(StundentafelFormSchema),
    defaultValues: { name: "", grade_level: 1, school_type: "Grundschule" },
  });
  const createMutation = useCreateStundentafel();

  async function handleStundentafelCreate(values: StundentafelFormValues) {
    const body: StundentafelCreate = {
      name: values.name,
      grade_level: values.grade_level,
      school_type: values.school_type,
    };
    try {
      await createMutation.mutateAsync(body);
      form.reset();
      onOpenChange(false);
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        form.setError("root", { message: t("stundentafeln.errors.duplicateName") });
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
          <DialogTitle>{t("stundentafeln.dialog.createTitle")}</DialogTitle>
          <DialogDescription>{t("stundentafeln.dialog.createDescription")}</DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(handleStundentafelCreate)}>
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("stundentafeln.fields.nameLabel")}</FormLabel>
                  <FormControl>
                    <Input placeholder={t("stundentafeln.fields.namePlaceholder")} {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="school_type"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("stundentafeln.fields.schoolTypeLabel")}</FormLabel>
                  <Select value={field.value} onValueChange={field.onChange}>
                    <FormControl>
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      {SchoolTypeValues.map((value) => (
                        <SelectItem key={value} value={value}>
                          {t(schoolTypeLabelKey(value))}
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
              name="grade_level"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("stundentafeln.fields.gradeLevelLabel")}</FormLabel>
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
            {rootError ? (
              <p role="alert" className="text-sm font-medium text-destructive">
                {rootError}
              </p>
            ) : null}
            <DialogFooter>
              <Button type="submit" disabled={createMutation.isPending}>
                {createMutation.isPending ? t("common.saving") : t("common.create")}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}

interface StundentafelEditDialogProps {
  stundentafel: Stundentafel;
  onClose: () => void;
}

export function StundentafelEditDialog({ stundentafel, onClose }: StundentafelEditDialogProps) {
  const { t } = useTranslation();
  const detail = useStundentafel(stundentafel.id);
  const updateMutation = useUpdateStundentafel();
  const form = useForm<StundentafelFormValues>({
    resolver: zodResolver(StundentafelFormSchema),
    defaultValues: {
      name: stundentafel.name,
      grade_level: stundentafel.grade_level,
      school_type: stundentafel.school_type,
    },
  });

  const [entryDialogMode, setEntryDialogMode] = useState<
    { mode: "create" } | { mode: "edit"; entry: StundentafelEntry } | null
  >(null);
  const [confirmEntryDelete, setConfirmEntryDelete] = useState<StundentafelEntry | null>(null);

  async function handleStundentafelSave(values: StundentafelFormValues) {
    const body: StundentafelUpdate = {
      name: values.name,
      grade_level: values.grade_level,
      school_type: values.school_type,
    };
    try {
      await updateMutation.mutateAsync({ id: stundentafel.id, body });
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        form.setError("root", { message: t("stundentafeln.errors.duplicateName") });
        return;
      }
      throw err;
    }
  }

  const rootError = form.formState.errors.root?.message;
  const entries = detail.data?.entries ?? [];
  const usedSubjectIds = new Set(entries.map((entry) => entry.subject.id));

  return (
    <Dialog open onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("stundentafeln.dialog.editTitle")}</DialogTitle>
          <DialogDescription>
            {t("stundentafeln.dialog.editDescription", { name: stundentafel.name })}
          </DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(handleStundentafelSave)}>
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("stundentafeln.fields.nameLabel")}</FormLabel>
                  <FormControl>
                    <Input placeholder={t("stundentafeln.fields.namePlaceholder")} {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="school_type"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("stundentafeln.fields.schoolTypeLabel")}</FormLabel>
                  <Select value={field.value} onValueChange={field.onChange}>
                    <FormControl>
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      {SchoolTypeValues.map((value) => (
                        <SelectItem key={value} value={value}>
                          {t(schoolTypeLabelKey(value))}
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
              name="grade_level"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("stundentafeln.fields.gradeLevelLabel")}</FormLabel>
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
            {rootError ? (
              <p role="alert" className="text-sm font-medium text-destructive">
                {rootError}
              </p>
            ) : null}
            <div className="flex justify-end">
              <Button type="submit" size="sm" disabled={updateMutation.isPending}>
                {updateMutation.isPending ? t("common.saving") : t("common.save")}
              </Button>
            </div>
          </form>
        </Form>

        <div className="border-t pt-4">
          <div className="flex items-center justify-between pb-2">
            <h3 className="text-sm font-semibold">{t("stundentafeln.entries.sectionTitle")}</h3>
            <Button size="sm" onClick={() => setEntryDialogMode({ mode: "create" })}>
              {t("stundentafeln.entries.add")}
            </Button>
          </div>

          {detail.isLoading ? (
            <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
          ) : detail.isError ? (
            <p className="text-sm text-destructive">{t("stundentafeln.entries.loadError")}</p>
          ) : entries.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("stundentafeln.entries.empty")}</p>
          ) : (
            <div className="rounded-md border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="py-2">
                      {t("stundentafeln.entries.columns.subject")}
                    </TableHead>
                    <TableHead className="py-2 text-right">
                      {t("stundentafeln.entries.columns.hoursPerWeek")}
                    </TableHead>
                    <TableHead className="py-2">
                      {t("stundentafeln.entries.columns.blockSize")}
                    </TableHead>
                    <TableHead className="w-40 py-2 text-right">{t("common.actions")}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {entries.map((entry) => (
                    <TableRow key={entry.id}>
                      <TableCell className="py-1.5 font-medium">
                        {entry.subject.name}{" "}
                        <span className="text-muted-foreground">· {entry.subject.short_name}</span>
                      </TableCell>
                      <TableCell className="py-1.5 text-right font-mono text-[12.5px]">
                        {entry.hours_per_week}
                      </TableCell>
                      <TableCell className="py-1.5">
                        {entry.preferred_block_size === 2
                          ? t("stundentafeln.fields.blockSizeDouble")
                          : t("stundentafeln.fields.blockSizeSingle")}
                      </TableCell>
                      <TableCell className="space-x-2 whitespace-nowrap py-1.5 text-right">
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => setEntryDialogMode({ mode: "edit", entry })}
                        >
                          {t("common.edit")}
                        </Button>
                        <Button
                          size="sm"
                          variant="destructive"
                          onClick={() => setConfirmEntryDelete(entry)}
                        >
                          {t("common.delete")}
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("stundentafeln.dialog.close")}
          </Button>
        </DialogFooter>

        {entryDialogMode ? (
          <EntryFormDialog
            tafelId={stundentafel.id}
            mode={entryDialogMode}
            usedSubjectIds={usedSubjectIds}
            onClose={() => setEntryDialogMode(null)}
          />
        ) : null}
        {confirmEntryDelete ? (
          <DeleteEntryDialog
            tafelId={stundentafel.id}
            entry={confirmEntryDelete}
            onClose={() => setConfirmEntryDelete(null)}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

interface EntryFormDialogProps {
  tafelId: string;
  mode: { mode: "create" } | { mode: "edit"; entry: StundentafelEntry };
  usedSubjectIds: Set<string>;
  onClose: () => void;
}

function EntryFormDialog({ tafelId, mode, usedSubjectIds, onClose }: EntryFormDialogProps) {
  const { t } = useTranslation();
  const subjects = useSubjects();
  const createMutation = useCreateStundentafelEntry(tafelId);
  const updateMutation = useUpdateStundentafelEntry(tafelId);

  const isEdit = mode.mode === "edit";
  const form = useForm<EntryFormValues>({
    resolver: zodResolver(EntryFormSchema),
    defaultValues: {
      subject_id: isEdit ? mode.entry.subject.id : "",
      hours_per_week: isEdit ? mode.entry.hours_per_week : 1,
      preferred_block_size: isEdit ? mode.entry.preferred_block_size : 1,
    },
  });

  const availableSubjects = (subjects.data ?? []).filter(
    (subject) => !usedSubjectIds.has(subject.id),
  );
  const noSubjectsAvailable = !isEdit && availableSubjects.length === 0;
  const submitting = createMutation.isPending || updateMutation.isPending;

  async function handleEntrySubmit(values: EntryFormValues) {
    try {
      if (mode.mode === "edit") {
        const body: EntryUpdate = {
          hours_per_week: values.hours_per_week,
          preferred_block_size: values.preferred_block_size,
        };
        await updateMutation.mutateAsync({ entryId: mode.entry.id, body });
      } else {
        const body: EntryCreate = {
          subject_id: values.subject_id,
          hours_per_week: values.hours_per_week,
          preferred_block_size: values.preferred_block_size,
        };
        await createMutation.mutateAsync(body);
      }
      onClose();
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        form.setError("root", { message: t("stundentafeln.errors.duplicateSubject") });
        return;
      }
      throw err;
    }
  }

  const rootError = form.formState.errors.root?.message;
  const entrySubjectName =
    mode.mode === "edit" ? `${mode.entry.subject.name} · ${mode.entry.subject.short_name}` : "";

  return (
    <Dialog open onOpenChange={(next) => !next && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {isEdit ? t("stundentafeln.entries.editTitle") : t("stundentafeln.entries.createTitle")}
          </DialogTitle>
          <DialogDescription>
            {isEdit ? entrySubjectName : t("stundentafeln.entries.add")}
          </DialogDescription>
        </DialogHeader>
        {noSubjectsAvailable ? (
          <div
            role="status"
            aria-live="polite"
            className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm text-foreground"
          >
            <p>{t("stundentafeln.entries.allSubjectsAssigned")}</p>
          </div>
        ) : null}
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(handleEntrySubmit)}>
            {isEdit ? (
              <div>
                <p className="text-sm font-medium">{t("stundentafeln.fields.subjectLabel")}</p>
                <p className="text-sm text-muted-foreground">{entrySubjectName}</p>
              </div>
            ) : (
              <FormField
                control={form.control}
                name="subject_id"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{t("stundentafeln.fields.subjectLabel")}</FormLabel>
                    <Select value={field.value} onValueChange={field.onChange}>
                      <FormControl>
                        <SelectTrigger>
                          <SelectValue placeholder={t("stundentafeln.fields.subjectPlaceholder")} />
                        </SelectTrigger>
                      </FormControl>
                      <SelectContent>
                        {availableSubjects.map((option) => (
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
            )}
            <FormField
              control={form.control}
              name="hours_per_week"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("stundentafeln.fields.hoursPerWeekLabel")}</FormLabel>
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
                  <FormLabel>{t("stundentafeln.fields.blockSizeLabel")}</FormLabel>
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
                      <SelectItem value="1">{t("stundentafeln.fields.blockSizeSingle")}</SelectItem>
                      <SelectItem value="2">{t("stundentafeln.fields.blockSizeDouble")}</SelectItem>
                    </SelectContent>
                  </Select>
                  <p className="text-xs text-muted-foreground">
                    {t("stundentafeln.fields.blockSizeHelp")}
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
              <Button variant="outline" onClick={onClose} type="button">
                {t("common.cancel")}
              </Button>
              <Button type="submit" disabled={submitting || noSubjectsAvailable}>
                {submitting ? t("common.saving") : isEdit ? t("common.save") : t("common.create")}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}

interface DeleteStundentafelDialogProps {
  stundentafel: Stundentafel;
  onClose: () => void;
}

export function DeleteStundentafelDialog({ stundentafel, onClose }: DeleteStundentafelDialogProps) {
  const { t } = useTranslation();
  const mutation = useDeleteStundentafel();
  return (
    <ConfirmDialog
      open
      onClose={onClose}
      title={t("stundentafeln.dialog.deleteTitle")}
      description={t("stundentafeln.dialog.deleteDescription", { name: stundentafel.name })}
      isPending={mutation.isPending}
      onConfirm={async () => {
        await mutation.mutateAsync(stundentafel.id);
        onClose();
      }}
    />
  );
}

interface DeleteEntryDialogProps {
  tafelId: string;
  entry: StundentafelEntry;
  onClose: () => void;
}

function DeleteEntryDialog({ tafelId, entry, onClose }: DeleteEntryDialogProps) {
  const { t } = useTranslation();
  const mutation = useDeleteStundentafelEntry(tafelId);
  return (
    <ConfirmDialog
      open
      onClose={onClose}
      title={t("stundentafeln.entries.deleteTitle")}
      description={t("stundentafeln.entries.deleteDescription", {
        subjectName: entry.subject.name,
      })}
      isPending={mutation.isPending}
      onConfirm={async () => {
        await mutation.mutateAsync(entry.id);
        onClose();
      }}
    />
  );
}
