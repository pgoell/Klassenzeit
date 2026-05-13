import { zodResolver } from "@hookform/resolvers/zod";
import { Plus } from "lucide-react";
import { Fragment, useState } from "react";
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
import { dayLongKey, dayShortKey } from "@/i18n/day-keys";
import { ApiError } from "@/lib/api-client";
import { cn } from "@/lib/utils";
import {
  type TimeBlock,
  useCreateTimeBlock,
  useDeleteTimeBlock,
  useUpdateTimeBlock,
  useWeekSchemeDetail,
} from "./hooks";
import { TimeBlockFormSchema, type TimeBlockFormValues } from "./schema";

const DAY_INDICES = [0, 1, 2, 3, 4] as const;
const DEFAULT_MIN_ROWS = 8;

type CreateMode = {
  mode: "create";
  day: number;
  position: number;
  defaultStart?: string;
  defaultEnd?: string;
};
type EditMode = { mode: "edit"; block: TimeBlock };
type BlockDialogMode = CreateMode | EditMode;

function formatTimeBlockRange(block: TimeBlock): { start: string; end: string } {
  return { start: block.start_time.slice(0, 5), end: block.end_time.slice(0, 5) };
}

export function TimeBlocksGrid({ schemeId }: { schemeId: string }) {
  const { t } = useTranslation();
  const detail = useWeekSchemeDetail(schemeId);
  const [blockDialogMode, setBlockDialogMode] = useState<BlockDialogMode | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<TimeBlock | null>(null);

  const blocks = detail.data?.time_blocks ?? [];
  const blocksByCellKey = new Map<string, TimeBlock>(
    blocks.map((b) => [`${b.day_of_week}:${b.position}`, b]),
  );
  const maxExistingPosition = blocks.reduce((acc, b) => Math.max(acc, b.position), 0);
  const rowCount = Math.max(maxExistingPosition + 1, DEFAULT_MIN_ROWS);
  const positions = Array.from({ length: rowCount }, (_, i) => i + 1);

  function handleAdd(day: number, position: number) {
    const samePosition = blocks.find((b) => b.position === position);
    setBlockDialogMode({
      mode: "create",
      day,
      position,
      defaultStart: samePosition?.start_time.slice(0, 5),
      defaultEnd: samePosition?.end_time.slice(0, 5),
    });
  }

  function handleEdit(block: TimeBlock) {
    setBlockDialogMode({ mode: "edit", block });
  }

  function handleRequestDelete(block: TimeBlock) {
    setBlockDialogMode(null);
    setConfirmDelete(block);
  }

  return (
    <div className="space-y-2">
      <h3 className="text-sm font-semibold">{t("weekSchemes.timeBlocks.sectionTitle")}</h3>
      {detail.isLoading ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : (
        <div className="kz-ws-grid" style={{ gridTemplateColumns: "56px repeat(5, 1fr)" }}>
          <div className="kz-ws-cell" data-variant="header" />
          {DAY_INDICES.map((day) => (
            <div key={`head-${day}`} className="kz-ws-cell" data-variant="header">
              {t(dayShortKey(day))}
            </div>
          ))}
          {positions.map((position) => (
            <Fragment key={`row-${position}`}>
              <div className="kz-ws-cell" data-variant="time">
                <span className="font-mono text-xs">{position}</span>
              </div>
              {DAY_INDICES.map((day) => (
                <TimeBlocksGridCell
                  key={`${day}:${position}`}
                  day={day}
                  position={position}
                  block={blocksByCellKey.get(`${day}:${position}`)}
                  onAdd={handleAdd}
                  onEdit={handleEdit}
                />
              ))}
            </Fragment>
          ))}
        </div>
      )}
      {blockDialogMode ? (
        <TimeBlockFormDialog
          schemeId={schemeId}
          mode={blockDialogMode}
          onClose={() => setBlockDialogMode(null)}
          onRequestDelete={handleRequestDelete}
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

interface TimeBlocksGridCellProps {
  day: number;
  position: number;
  block: TimeBlock | undefined;
  onAdd: (day: number, position: number) => void;
  onEdit: (block: TimeBlock) => void;
}

function TimeBlocksGridCell({ day, position, block, onAdd, onEdit }: TimeBlocksGridCellProps) {
  const { t } = useTranslation();
  if (block) {
    const { start, end } = formatTimeBlockRange(block);
    return (
      <Button
        type="button"
        variant="ghost"
        className={cn(
          "kz-ws-cell-button flex h-auto min-h-12 flex-col items-center justify-center gap-0 px-1 py-1",
        )}
        aria-label={t("weekSchemes.timeBlocks.grid.filledCellLabel", {
          day: t(dayLongKey(day)),
          position,
        })}
        onClick={() => onEdit(block)}
      >
        <span className="font-mono text-xs">{start}</span>
        <span className="font-mono text-xs">{end}</span>
      </Button>
    );
  }
  return (
    <Button
      type="button"
      variant="ghost"
      className={cn("kz-ws-cell-button flex h-auto min-h-12 items-center justify-center px-1 py-1")}
      aria-label={t("weekSchemes.timeBlocks.grid.emptyCellLabel", {
        day: t(dayLongKey(day)),
        position,
      })}
      onClick={() => onAdd(day, position)}
    >
      <Plus aria-hidden className="size-4 text-muted-foreground" />
    </Button>
  );
}

interface TimeBlockFormDialogProps {
  schemeId: string;
  mode: BlockDialogMode;
  onClose: () => void;
  onRequestDelete: (block: TimeBlock) => void;
}

function TimeBlockFormDialog({
  schemeId,
  mode,
  onClose,
  onRequestDelete,
}: TimeBlockFormDialogProps) {
  const { t } = useTranslation();
  const createMutation = useCreateTimeBlock(schemeId);
  const updateMutation = useUpdateTimeBlock(schemeId);
  const isEdit = mode.mode === "edit";
  const form = useForm<TimeBlockFormValues>({
    resolver: zodResolver(TimeBlockFormSchema),
    defaultValues: {
      day_of_week: isEdit ? mode.block.day_of_week : mode.day,
      position: isEdit ? mode.block.position : mode.position,
      start_time: isEdit ? mode.block.start_time.slice(0, 5) : (mode.defaultStart ?? "08:00"),
      end_time: isEdit ? mode.block.end_time.slice(0, 5) : (mode.defaultEnd ?? "08:45"),
    },
  });
  const submitting = createMutation.isPending || updateMutation.isPending;

  async function handleTimeBlockSubmit(values: TimeBlockFormValues) {
    const body = {
      day_of_week: values.day_of_week,
      position: values.position,
      start_time: `${values.start_time}:00`,
      end_time: `${values.end_time}:00`,
      kind: "lesson" as const,
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
                      {DAY_INDICES.map((day) => (
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
              {mode.mode === "edit" ? (
                <Button
                  variant="destructive"
                  type="button"
                  onClick={() => onRequestDelete(mode.block)}
                >
                  {t("weekSchemes.timeBlocks.dialog.deleteFooter")}
                </Button>
              ) : null}
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
