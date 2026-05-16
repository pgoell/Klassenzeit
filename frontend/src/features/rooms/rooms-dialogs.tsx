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
import { SubjectMultiPicker } from "@/features/subjects/subject-multi-picker";
import { ApiError } from "@/lib/api-client";
import {
  type Room,
  useCreateRoomWithSuitability,
  useDeleteRoom,
  useRoomDetail,
  useUpdateRoomWithSuitability,
} from "./hooks";
import { RoomAvailabilityGrid } from "./room-availability-grid";
import { RoomFormSchema, type RoomFormValues } from "./schema";

interface RoomFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  submitLabel: string;
  room?: Room;
}

export function RoomFormDialog({ open, onOpenChange, submitLabel, room }: RoomFormDialogProps) {
  const { t } = useTranslation();
  const detail = useRoomDetail(room ? room.id : null);
  const original = detail.data?.suitability_subjects.map((s) => s.id) ?? [];

  const form = useForm<RoomFormValues>({
    resolver: zodResolver(RoomFormSchema),
    defaultValues: {
      name: room?.name ?? "",
      short_name: room?.short_name ?? "",
      capacity: room?.capacity ?? undefined,
      is_external: room?.is_external ?? false,
      suitable_subject_ids: original,
    },
    values: room
      ? {
          name: room.name,
          short_name: room.short_name,
          capacity: room.capacity ?? undefined,
          is_external: room.is_external,
          suitable_subject_ids: original,
        }
      : undefined,
  });
  const createMutation = useCreateRoomWithSuitability();
  const updateMutation = useUpdateRoomWithSuitability();
  const submitting = createMutation.isPending || updateMutation.isPending;

  const title = room ? t("rooms.dialog.editTitle") : t("rooms.dialog.createTitle");
  const description = room
    ? t("rooms.dialog.editDescription", { name: room.name })
    : t("rooms.dialog.createDescription");

  async function handleRoomSubmit(values: RoomFormValues) {
    const capacity = typeof values.capacity === "number" ? values.capacity : null;
    try {
      if (room) {
        await updateMutation.mutateAsync({
          id: room.id,
          base: {
            name: values.name,
            short_name: values.short_name,
            capacity,
            is_external: values.is_external,
          },
          suitable_subject_ids: values.suitable_subject_ids,
          original_suitable_subject_ids: original,
        });
      } else {
        await createMutation.mutateAsync({
          base: {
            name: values.name,
            short_name: values.short_name,
            capacity,
            is_external: values.is_external,
          },
          suitable_subject_ids: values.suitable_subject_ids,
        });
      }
      form.reset();
      onOpenChange(false);
    } catch (err) {
      if (
        err instanceof ApiError &&
        err.status === 400 &&
        err.data &&
        typeof err.data === "object" &&
        "missing_subject_ids" in err.data
      ) {
        form.setError("suitable_subject_ids", {
          type: "custom",
          message: t("rooms.suitableSubjectsError"),
        });
        return;
      }
      throw err;
    }
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
          <form className="space-y-4" onSubmit={form.handleSubmit(handleRoomSubmit)}>
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("rooms.columns.name")}</FormLabel>
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
                  <FormLabel>{t("rooms.columns.shortName")}</FormLabel>
                  <FormControl>
                    <Input {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="capacity"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("rooms.columns.capacity")}</FormLabel>
                  <FormControl>
                    <Input
                      type="number"
                      min={1}
                      value={field.value ?? ""}
                      onChange={(e) =>
                        field.onChange(e.target.value === "" ? undefined : Number(e.target.value))
                      }
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="is_external"
              render={({ field }) => (
                <FormItem className="flex flex-row items-start space-x-3 space-y-0">
                  <FormControl>
                    <Checkbox
                      checked={field.value}
                      onCheckedChange={(next) => field.onChange(next === true)}
                    />
                  </FormControl>
                  <div className="space-y-1 leading-none">
                    <FormLabel>{t("rooms.fields.isExternal")}</FormLabel>
                    <FormDescription>{t("rooms.fields.isExternalDescription")}</FormDescription>
                  </div>
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="suitable_subject_ids"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("rooms.suitableSubjects")}</FormLabel>
                  <FormControl>
                    <SubjectMultiPicker value={field.value} onChange={field.onChange} />
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
        {room ? <RoomAvailabilityGrid roomId={room.id} /> : null}
      </DialogContent>
    </Dialog>
  );
}

interface DeleteRoomDialogProps {
  room: Room;
  onClose: () => void;
}

export function DeleteRoomDialog({ room, onClose }: DeleteRoomDialogProps) {
  const { t } = useTranslation();
  const mutation = useDeleteRoom();
  return (
    <ConfirmDialog
      open
      onClose={onClose}
      title={t("rooms.dialog.deleteTitle")}
      description={t("rooms.dialog.deleteDescription", { name: room.name })}
      isPending={mutation.isPending}
      onConfirm={async () => {
        await mutation.mutateAsync(room.id);
        onClose();
      }}
    />
  );
}
