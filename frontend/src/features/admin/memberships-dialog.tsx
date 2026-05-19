import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useSchools } from "@/features/schools/hooks";
import { ApiError } from "@/lib/api-client";
import {
  type AdminUser,
  type Membership,
  useGrantMembership,
  useRevokeMembership,
  useUserMemberships,
} from "./users-hooks";

interface MembershipsDialogProps {
  user: AdminUser;
  onClose: () => void;
}

type GrantErrorKey =
  | "adminUsers.toast.grantConflictExists"
  | "adminUsers.toast.grantConflictHome"
  | "adminUsers.toast.grantError";

function membershipErrorKey(err: unknown): GrantErrorKey {
  if (err instanceof ApiError) {
    const detail = err.data as { detail?: { code?: string } } | null;
    const code = detail?.detail?.code;
    if (code === "membership_exists") return "adminUsers.toast.grantConflictExists";
    if (code === "membership_redundant_home_school") return "adminUsers.toast.grantConflictHome";
  }
  return "adminUsers.toast.grantError";
}

export function MembershipsDialog({ user, onClose }: MembershipsDialogProps) {
  const { t } = useTranslation();
  const memberships = useUserMemberships(user.id);
  const schools = useSchools();
  const grant = useGrantMembership(user.id);
  const revoke = useRevokeMembership(user.id);
  const [selectedSchool, setSelectedSchool] = useState<string>("");
  const [removeTarget, setRemoveTarget] = useState<Membership | null>(null);

  const grantedIds = new Set(memberships.data?.map((m) => m.school_id) ?? []);
  const grantableSchools = (schools.data ?? []).filter(
    (s) => s.id !== user.school_id && !grantedIds.has(s.id),
  );

  async function handleGrant() {
    if (!selectedSchool) return;
    const targetName =
      grantableSchools.find((s) => s.id === selectedSchool)?.name ?? selectedSchool;
    try {
      await grant.mutateAsync(selectedSchool);
      toast.success(t("adminUsers.toast.grantSuccess", { schoolName: targetName }));
      setSelectedSchool("");
    } catch (err) {
      toast.error(t(membershipErrorKey(err)));
    }
  }

  async function handleRevokeConfirm() {
    if (!removeTarget) return;
    const targetName = removeTarget.school_name;
    try {
      await revoke.mutateAsync(removeTarget.school_id);
      toast.success(t("adminUsers.toast.revokeSuccess", { schoolName: targetName }));
    } catch {
      toast.error(t("adminUsers.toast.revokeError"));
    } finally {
      setRemoveTarget(null);
    }
  }

  return (
    <Dialog
      open
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("adminUsers.dialog.title", { email: user.email })}</DialogTitle>
          <DialogDescription>{t("adminUsers.dialog.description")}</DialogDescription>
        </DialogHeader>

        <section className="space-y-2">
          <h3 className="text-sm font-medium text-muted-foreground">
            {t("adminUsers.dialog.homeSchool")}
          </h3>
          <p className="rounded-md border bg-muted px-3 py-2 text-sm">{user.school_name}</p>
        </section>

        <section className="space-y-2">
          <h3 className="text-sm font-medium text-muted-foreground">
            {t("adminUsers.dialog.memberships")}
          </h3>
          {memberships.isLoading ? (
            <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
          ) : (memberships.data ?? []).length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("adminUsers.dialog.noMemberships")}</p>
          ) : (
            <ul className="space-y-2">
              {(memberships.data ?? []).map((m) => (
                <li
                  key={m.school_id}
                  className="flex items-center justify-between rounded-md border px-3 py-2 text-sm"
                >
                  <span>{m.school_name}</span>
                  <Button size="sm" variant="outline" onClick={() => setRemoveTarget(m)}>
                    {t("common.remove")}
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="space-y-2">
          <h3 className="text-sm font-medium text-muted-foreground">
            {t("adminUsers.dialog.grant")}
          </h3>
          <div className="flex items-center gap-2">
            <Select value={selectedSchool} onValueChange={setSelectedSchool}>
              <SelectTrigger aria-label={t("adminUsers.dialog.pickSchool")} className="flex-1">
                <SelectValue placeholder={t("adminUsers.dialog.pickSchool")} />
              </SelectTrigger>
              <SelectContent>
                {grantableSchools.map((s) => (
                  <SelectItem key={s.id} value={s.id}>
                    {s.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              disabled={!selectedSchool || grant.isPending}
              onClick={() => {
                void handleGrant();
              }}
            >
              {t("adminUsers.dialog.grantButton")}
            </Button>
          </div>
        </section>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("common.cancel")}
          </Button>
        </DialogFooter>
      </DialogContent>

      <ConfirmDialog
        open={removeTarget !== null}
        onClose={() => setRemoveTarget(null)}
        title={t("adminUsers.dialog.confirmRemoveTitle")}
        description={t("adminUsers.dialog.confirmRemoveBody", {
          schoolName: removeTarget?.school_name ?? "",
        })}
        confirmLabel={t("common.remove")}
        isPending={revoke.isPending}
        onConfirm={handleRevokeConfirm}
      />
    </Dialog>
  );
}
