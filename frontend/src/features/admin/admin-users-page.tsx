import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { type EntityColumn, EntityListTable } from "@/components/entity-list-table";
import { Button } from "@/components/ui/button";
import { MembershipsDialog } from "./memberships-dialog";
import { type AdminUser, useAdminUsers } from "./users-hooks";

function formatRole(role: string, t: TFunction): string {
  if (role === "user") return t("adminUsers.roles.user");
  if (role === "admin") return t("adminUsers.roles.admin");
  if (role === "super_admin") return t("adminUsers.roles.super_admin");
  return role;
}

function formatLastLogin(value: string | null, locale: string, never: string): string {
  if (!value) return never;
  const d = new Date(value);
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(d);
}

export function AdminUsersPage() {
  const { t, i18n } = useTranslation();
  const users = useAdminUsers();
  const [managing, setManaging] = useState<AdminUser | null>(null);

  const columns: EntityColumn<AdminUser>[] = [
    {
      key: "email",
      header: t("adminUsers.columns.email"),
      cell: (u) => u.email,
      cellClassName: "font-medium",
    },
    {
      key: "role",
      header: t("adminUsers.columns.role"),
      cell: (u) => formatRole(u.role, t),
    },
    {
      key: "homeSchool",
      header: t("adminUsers.columns.homeSchool"),
      cell: (u) => u.school_name,
    },
    {
      key: "active",
      header: t("adminUsers.columns.active"),
      cell: (u) => (u.is_active ? t("adminUsers.activeYes") : t("adminUsers.activeNo")),
    },
    {
      key: "lastLogin",
      header: t("adminUsers.columns.lastLogin"),
      cell: (u) => formatLastLogin(u.last_login_at, i18n.language, t("adminUsers.never")),
    },
  ];

  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">{t("adminUsers.title")}</h1>
        <p className="mt-1 text-sm text-muted-foreground">{t("adminUsers.subtitle")}</p>
      </div>
      {users.isLoading ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : users.isError ? (
        <p className="text-sm text-destructive">{t("adminUsers.loadError")}</p>
      ) : (users.data ?? []).length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("adminUsers.empty")}</p>
      ) : (
        <EntityListTable<AdminUser>
          rows={users.data ?? []}
          rowKey={(u) => u.id}
          columns={columns}
          actions={(u) => (
            <Button size="sm" variant="outline" onClick={() => setManaging(u)}>
              {t("adminUsers.manageSchools")}
            </Button>
          )}
          actionsHeader={t("common.actions")}
        />
      )}

      {managing ? <MembershipsDialog user={managing} onClose={() => setManaging(null)} /> : null}
    </div>
  );
}
