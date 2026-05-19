import { useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useAdminUsers } from "@/features/admin/users-hooks";
import { useSchools } from "@/features/schools/hooks";
import type { AuditLogSearch } from "./search";

const NULL_VALUE = "__none__";

type Props = {
  search: AuditLogSearch;
};

export function AuditLogFilters({ search }: Props) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const schools = useSchools();
  const users = useAdminUsers();

  function patch(next: Partial<AuditLogSearch>) {
    void navigate({
      to: "/audit-log",
      search: { ...search, skip: 0, ...next },
    });
  }

  return (
    <div className="flex flex-wrap items-end gap-3">
      <div className="flex flex-col gap-1">
        <Label htmlFor="audit-actor">{t("auditLog.filters.actor")}</Label>
        <Select
          value={search.actor_user_id ?? NULL_VALUE}
          onValueChange={(v) => patch({ actor_user_id: v === NULL_VALUE ? undefined : v })}
        >
          <SelectTrigger id="audit-actor" className="w-56">
            <SelectValue placeholder={t("auditLog.filters.actorAll")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={NULL_VALUE}>{t("auditLog.filters.actorAll")}</SelectItem>
            {(users.data ?? []).map((u) => (
              <SelectItem key={u.id} value={u.id}>
                {u.email}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="flex flex-col gap-1">
        <Label htmlFor="audit-school">{t("auditLog.filters.school")}</Label>
        <Select
          value={search.target_school_id ?? NULL_VALUE}
          onValueChange={(v) => patch({ target_school_id: v === NULL_VALUE ? undefined : v })}
        >
          <SelectTrigger id="audit-school" className="w-56">
            <SelectValue placeholder={t("auditLog.filters.schoolAll")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={NULL_VALUE}>{t("auditLog.filters.schoolAll")}</SelectItem>
            {(schools.data ?? []).map((s) => (
              <SelectItem key={s.id} value={s.id}>
                {s.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="flex flex-col gap-1">
        <Label htmlFor="audit-from">{t("auditLog.filters.from")}</Label>
        <Input
          id="audit-from"
          type="date"
          value={search.from_ts ? search.from_ts.slice(0, 10) : ""}
          onChange={(e) =>
            patch({
              from_ts: e.target.value ? new Date(e.target.value).toISOString() : undefined,
            })
          }
        />
      </div>
      <div className="flex flex-col gap-1">
        <Label htmlFor="audit-to">{t("auditLog.filters.to")}</Label>
        <Input
          id="audit-to"
          type="date"
          value={search.to_ts ? search.to_ts.slice(0, 10) : ""}
          onChange={(e) =>
            patch({
              to_ts: e.target.value ? new Date(e.target.value).toISOString() : undefined,
            })
          }
        />
      </div>
      <Button
        variant="outline"
        onClick={() =>
          patch({
            actor_user_id: undefined,
            target_school_id: undefined,
            from_ts: undefined,
            to_ts: undefined,
          })
        }
      >
        {t("auditLog.filters.clear")}
      </Button>
    </div>
  );
}
