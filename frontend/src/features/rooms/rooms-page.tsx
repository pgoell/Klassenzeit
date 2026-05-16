import { useSearch } from "@tanstack/react-router";
import { DoorOpen, ExternalLink } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/empty-state";
import { type EntityColumn, EntityListTable } from "@/components/entity-list-table";
import { EntityPageHead } from "@/components/entity-page-head";
import { Toolbar } from "@/components/toolbar";
import { Button } from "@/components/ui/button";
import { type Room, useRooms } from "./hooks";
import { DeleteRoomDialog, RoomFormDialog } from "./rooms-dialogs";

export function RoomsPage() {
  const { t } = useTranslation();
  const rooms = useRooms();
  const search = useSearch({ strict: false }) as { create?: string };

  const [q, setQ] = useState("");
  const [creating, setCreating] = useState(() => search.create === "1");
  const [editing, setEditing] = useState<Room | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Room | null>(null);

  const rows = (rooms.data ?? []).filter((row) =>
    q ? `${row.name} ${row.short_name}`.toLowerCase().includes(q.toLowerCase()) : true,
  );
  const showEmpty = !rooms.isLoading && rooms.data && rooms.data.length === 0 && !q;

  const roomColumns: EntityColumn<Room>[] = [
    {
      key: "name",
      header: t("rooms.columns.name"),
      cell: (room) => (
        <span className="inline-flex items-center gap-1.5">
          {room.name}
          {room.is_external ? (
            <span title={t("rooms.fields.isExternalDescription")} className="inline-flex">
              <ExternalLink
                className="h-3.5 w-3.5 text-muted-foreground"
                aria-label={t("rooms.fields.isExternal")}
              />
            </span>
          ) : null}
        </span>
      ),
      cellClassName: "font-medium",
    },
    {
      key: "shortName",
      header: t("rooms.columns.shortName"),
      cell: (room) => room.short_name,
      cellClassName: "font-mono text-[12.5px]",
    },
    {
      key: "capacity",
      header: t("rooms.columns.capacity"),
      cell: (room) => room.capacity ?? "—",
      className: "text-right",
      cellClassName: "font-mono text-[12.5px]",
    },
  ];

  return (
    <div className="space-y-4">
      <EntityPageHead
        title={t("rooms.title")}
        subtitle={t("rooms.subtitle")}
        onCreate={() => setCreating(true)}
        createLabel={t("rooms.new")}
      />

      {rooms.isLoading ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : rooms.isError ? (
        <p className="text-sm text-destructive">{t("rooms.loadError")}</p>
      ) : showEmpty ? (
        <EmptyState
          icon={<DoorOpen className="h-7 w-7" />}
          title={t("rooms.empty.title")}
          body={t("rooms.empty.body")}
          steps={[t("rooms.empty.step1"), t("rooms.empty.step2"), t("rooms.empty.step3")]}
          onCreate={() => setCreating(true)}
          createLabel={t("rooms.new")}
        />
      ) : (
        <>
          <Toolbar
            search={q}
            onSearch={setQ}
            placeholder={t("common.search")}
            right={
              <span className="font-mono text-xs text-muted-foreground">
                {rows.length} {t("rooms.title").toLowerCase()}
              </span>
            }
          />
          <EntityListTable<Room>
            rows={rows}
            rowKey={(room) => room.id}
            columns={roomColumns}
            actions={(room) => (
              <>
                <Button size="sm" variant="outline" onClick={() => setEditing(room)}>
                  {t("common.edit")}
                </Button>
                <Button size="sm" variant="destructive" onClick={() => setConfirmDelete(room)}>
                  {t("common.delete")}
                </Button>
              </>
            )}
            actionsHeader={t("common.actions")}
          />
        </>
      )}

      <RoomFormDialog open={creating} onOpenChange={setCreating} submitLabel={t("common.create")} />
      {editing ? (
        <RoomFormDialog
          open={true}
          room={editing}
          onOpenChange={(open) => {
            if (!open) setEditing(null);
          }}
          submitLabel={t("common.save")}
        />
      ) : null}
      {confirmDelete ? (
        <DeleteRoomDialog room={confirmDelete} onClose={() => setConfirmDelete(null)} />
      ) : null}
    </div>
  );
}
