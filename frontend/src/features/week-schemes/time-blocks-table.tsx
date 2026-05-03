import { zodResolver } from "@hookform/resolvers/zod";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
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
import { dayLongKey } from "@/i18n/day-keys";
import { ApiError } from "@/lib/api-client";
import {
  type TimeBlock,
  useCreateTimeBlock,
  useDeleteTimeBlock,
  useUpdateTimeBlock,
  useWeekSchemeDetail,
} from "./hooks";
import { TimeBlockFormSchema, type TimeBlockFormValues } from "./schema";

export function TimeBlocksTable({ schemeId }: { schemeId: string }) {
  const { t } = useTranslation();
  const detail = useWeekSchemeDetail(schemeId);
  const [blockDialogMode, setBlockDialogMode] = useState<
    { mode: "create" } | { mode: "edit"; block: TimeBlock } | null
  >(null);
  const [confirmDelete, setConfirmDelete] = useState<TimeBlock | null>(null);

  const blocks = [...(detail.data?.time_blocks ?? [])].sort(
    (a, b) => a.day_of_week - b.day_of_week || a.position - b.position,
  );

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between pb-2">
        <h3 className="text-sm font-semibold">{t("weekSchemes.timeBlocks.sectionTitle")}</h3>
        <Button size="sm" onClick={() => setBlockDialogMode({ mode: "create" })}>
          {t("weekSchemes.timeBlocks.add")}
        </Button>
      </div>
      {detail.isLoading ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : blocks.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("weekSchemes.timeBlocks.empty")}</p>
      ) : (
        <div className="overflow-x-auto rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="py-2">{t("weekSchemes.timeBlocks.columns.day")}</TableHead>
                <TableHead className="py-2 text-right">
                  {t("weekSchemes.timeBlocks.columns.position")}
                </TableHead>
                <TableHead className="py-2">{t("weekSchemes.timeBlocks.columns.start")}</TableHead>
                <TableHead className="py-2">{t("weekSchemes.timeBlocks.columns.end")}</TableHead>
                <TableHead className="py-2 text-right">{t("common.actions")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {blocks.map((block) => (
                <TableRow key={block.id}>
                  <TableCell className="py-1.5 font-medium">
                    {t(dayLongKey(block.day_of_week))}
                  </TableCell>
                  <TableCell className="py-1.5 text-right font-mono text-[12.5px]">
                    {block.position}
                  </TableCell>
                  <TableCell className="py-1.5 font-mono text-[12.5px]">
                    {block.start_time}
                  </TableCell>
                  <TableCell className="py-1.5 font-mono text-[12.5px]">{block.end_time}</TableCell>
                  <TableCell className="py-1.5 text-right whitespace-nowrap">
                    <div className="inline-flex gap-2">
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => setBlockDialogMode({ mode: "edit", block })}
                      >
                        {t("common.edit")}
                      </Button>
                      <Button
                        size="sm"
                        variant="destructive"
                        onClick={() => setConfirmDelete(block)}
                      >
                        {t("common.delete")}
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
      {blockDialogMode ? (
        <TimeBlockFormDialog
          schemeId={schemeId}
          mode={blockDialogMode}
          onClose={() => setBlockDialogMode(null)}
        />
      ) : null}
      {confirmDelete ? (
        <DeleteTimeBlockDialog
          schemeId={schemeId}
          block={confirmDelete}
          onClose={() => setConfirmDelete(null)}
        />
      ) : null}
    </div>
  );
}

interface TimeBlockFormDialogProps {
  schemeId: string;
  mode: { mode: "create" } | { mode: "edit"; block: TimeBlock };
  onClose: () => void;
}

function TimeBlockFormDialog({ schemeId, mode, onClose }: TimeBlockFormDialogProps) {
  const { t } = useTranslation();
  const createMutation = useCreateTimeBlock(schemeId);
  const updateMutation = useUpdateTimeBlock(schemeId);
  const isEdit = mode.mode === "edit";
  const form = useForm<TimeBlockFormValues>({
    resolver: zodResolver(TimeBlockFormSchema),
    defaultValues: {
      day_of_week: isEdit ? mode.block.day_of_week : 0,
      position: isEdit ? mode.block.position : 1,
      start_time: isEdit ? mode.block.start_time.slice(0, 5) : "08:00",
      end_time: isEdit ? mode.block.end_time.slice(0, 5) : "08:45",
    },
  });
  const submitting = createMutation.isPending || updateMutation.isPending;

  async function handleTimeBlockSubmit(values: TimeBlockFormValues) {
    const body = {
      day_of_week: values.day_of_week,
      position: values.position,
      start_time: `${values.start_time}:00`,
      end_time: `${values.end_time}:00`,
    };
    try {
      if (mode.mode === "edit") {
        await updateMutation.mutateAsync({ blockId: mode.block.id, body });
      } else {
        await createMutation.mutateAsync(body);
      }
      onClose();
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        form.setError("root", { message: t("weekSchemes.timeBlocks.errors.duplicate") });
        return;
      }
      throw err;
    }
  }

  const rootError = form.formState.errors.root?.message;

  return (
    <Dialog open onOpenChange={(next) => !next && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {isEdit
              ? t("weekSchemes.timeBlocks.editTitle")
              : t("weekSchemes.timeBlocks.createTitle")}
          </DialogTitle>
          <DialogDescription>{t("weekSchemes.timeBlocks.sectionTitle")}</DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(handleTimeBlockSubmit)}>
            <FormField
              control={form.control}
              name="day_of_week"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("weekSchemes.timeBlocks.columns.day")}</FormLabel>
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
                      {[0, 1, 2, 3, 4].map((day) => (
                        <SelectItem key={day} value={String(day)}>
                          {t(dayLongKey(day))}
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
              name="position"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("common.position")}</FormLabel>
                  <FormControl>
                    <Input
                      type="number"
                      min={1}
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
              name="start_time"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("common.start")}</FormLabel>
                  <FormControl>
                    <Input type="time" {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="end_time"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("common.end")}</FormLabel>
                  <FormControl>
                    <Input type="time" {...field} />
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
              <Button variant="outline" type="button" onClick={onClose}>
                {t("common.cancel")}
              </Button>
              <Button type="submit" disabled={submitting}>
                {submitting ? t("common.saving") : isEdit ? t("common.save") : t("common.create")}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}

interface DeleteTimeBlockDialogProps {
  schemeId: string;
  block: TimeBlock;
  onClose: () => void;
}

function DeleteTimeBlockDialog({ schemeId, block, onClose }: DeleteTimeBlockDialogProps) {
  const { t } = useTranslation();
  const mutation = useDeleteTimeBlock(schemeId);
  async function confirmTimeBlockDelete() {
    await mutation.mutateAsync(block.id);
    onClose();
  }
  return (
    <Dialog open onOpenChange={(next) => !next && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("weekSchemes.timeBlocks.deleteTitle")}</DialogTitle>
          <DialogDescription>
            {t("weekSchemes.timeBlocks.deleteDescription", {
              day: t(dayLongKey(block.day_of_week)),
              position: block.position,
            })}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="destructive"
            onClick={confirmTimeBlockDelete}
            disabled={mutation.isPending}
          >
            {mutation.isPending ? t("common.deleting") : t("common.delete")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
