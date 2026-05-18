import { Link } from "@tanstack/react-router";
import {
  BookOpen,
  Calendar,
  CalendarDays,
  ClipboardList,
  DoorOpen,
  GraduationCap,
  Layers,
  LayoutDashboard,
  LogOut,
  type LucideIcon,
  PanelLeft,
  ShieldCheck,
  Users,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useSidebar } from "@/components/sidebar-provider";
import { Button } from "@/components/ui/button";
import { useLogout, useMe } from "@/lib/auth";
import { cn } from "@/lib/utils";

type NavLabelKey =
  | "nav.dashboard"
  | "nav.schedule"
  | "nav.subjects"
  | "nav.rooms"
  | "nav.teachers"
  | "nav.weekSchemes"
  | "sidebar.schoolClasses"
  | "sidebar.stundentafeln"
  | "sidebar.lessons"
  | "sidebar.schools";

type GroupLabelKey = "sidebar.main" | "sidebar.data" | "sidebar.admin";

interface NavItem {
  to: string;
  labelKey: NavLabelKey;
  icon: LucideIcon;
  disabled?: boolean;
}

interface NavGroup {
  labelKey: GroupLabelKey;
  items: NavItem[];
}

const NAV_GROUPS: NavGroup[] = [
  {
    labelKey: "sidebar.main",
    items: [
      { to: "/", labelKey: "nav.dashboard", icon: LayoutDashboard },
      { to: "/schedule", labelKey: "nav.schedule", icon: Calendar },
    ],
  },
  {
    labelKey: "sidebar.data",
    items: [
      { to: "/subjects", labelKey: "nav.subjects", icon: BookOpen },
      { to: "/rooms", labelKey: "nav.rooms", icon: DoorOpen },
      { to: "/teachers", labelKey: "nav.teachers", icon: GraduationCap },
      { to: "/week-schemes", labelKey: "nav.weekSchemes", icon: CalendarDays },
      { to: "/school-classes", labelKey: "sidebar.schoolClasses", icon: Users },
      { to: "/stundentafeln", labelKey: "sidebar.stundentafeln", icon: ClipboardList },
      { to: "/lessons", labelKey: "sidebar.lessons", icon: Layers },
    ],
  },
];

const ADMIN_NAV_GROUP: NavGroup = {
  labelKey: "sidebar.admin",
  items: [{ to: "/schools", labelKey: "sidebar.schools", icon: ShieldCheck }],
};

export function AppSidebar() {
  const { t } = useTranslation();
  const { collapsed, toggle } = useSidebar();
  const me = useMe();
  const logout = useLogout();
  const navGroups =
    me.data?.role === "admin" || me.data?.role === "super_admin"
      ? [...NAV_GROUPS, ADMIN_NAV_GROUP]
      : NAV_GROUPS;

  const toggleLabel = collapsed ? t("sidebar.expand") : t("sidebar.collapse");

  return (
    <aside
      data-collapsed={collapsed ? "true" : "false"}
      className={cn(
        "flex flex-col border-r bg-sidebar text-sidebar-foreground transition-[width] duration-200 ease-out",
        collapsed ? "w-14 px-2 py-5" : "w-60 px-4 py-5",
      )}
    >
      <div className="flex items-center gap-2 pb-4">
        {!collapsed ? (
          <>
            <div className="kz-brand-mark">KZ</div>
            <span className="text-base font-semibold tracking-tight">Klassenzeit</span>
          </>
        ) : null}
        <Button
          variant="ghost"
          size="icon"
          onClick={toggle}
          aria-label={toggleLabel}
          title={toggleLabel}
          className={cn(collapsed ? "mx-auto" : "ml-auto")}
        >
          <PanelLeft className="h-4 w-4" />
        </Button>
      </div>

      {navGroups.map((group) => (
        <nav key={group.labelKey} className="flex flex-col gap-1 pb-3">
          {!collapsed ? (
            <div className="px-2 pt-2 pb-1 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              {t(group.labelKey)}
            </div>
          ) : (
            <div className="h-2" />
          )}
          {group.items.map((item) => {
            const label = t(item.labelKey);
            const Icon = item.icon;
            const base =
              "flex items-center gap-2 rounded-md px-2 py-2 text-sm font-medium hover:bg-sidebar-accent hover:text-sidebar-accent-foreground";
            if (item.disabled) {
              return (
                <span
                  key={item.labelKey}
                  title={t("sidebar.comingSoon")}
                  className={cn(
                    base,
                    "cursor-not-allowed opacity-50",
                    collapsed && "justify-center",
                  )}
                  aria-disabled="true"
                >
                  <Icon className="h-4 w-4" />
                  {!collapsed ? <span>{label}</span> : null}
                </span>
              );
            }
            return (
              <Link
                key={item.labelKey}
                to={item.to}
                className={cn(base, collapsed && "justify-center")}
                activeOptions={{ exact: item.to === "/" }}
                activeProps={{
                  className: "bg-sidebar-accent text-sidebar-accent-foreground",
                }}
                title={collapsed ? label : undefined}
              >
                <Icon className="h-4 w-4" />
                {!collapsed ? <span>{label}</span> : null}
              </Link>
            );
          })}
        </nav>
      ))}

      <div className="mt-auto border-t pt-3">
        {!collapsed ? (
          <div className="flex items-center gap-2 px-2 pb-2 text-sm">
            <div className="grid h-7 w-7 place-items-center rounded-full bg-accent text-accent-foreground text-xs font-semibold">
              {initials(me.data?.email)}
            </div>
            <div className="flex flex-col gap-0 leading-tight">
              <span className="text-xs text-muted-foreground">{me.data?.email ?? "…"}</span>
              {me.data?.school_name ? (
                <span className="text-[10px] text-muted-foreground/80">{me.data.school_name}</span>
              ) : null}
              {me.data?.role === "super_admin" ? (
                <span className="text-[10px] font-semibold uppercase tracking-wide text-primary">
                  {t("sidebar.superAdminBadge")}
                </span>
              ) : null}
            </div>
          </div>
        ) : null}
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            void logout.mutateAsync();
          }}
          disabled={logout.isPending}
          className={cn("w-full justify-start", collapsed && "justify-center px-0")}
          title={collapsed ? t("nav.logOut") : undefined}
        >
          <LogOut className="h-4 w-4" />
          {!collapsed ? <span className="ml-2">{t("nav.logOut")}</span> : null}
        </Button>
      </div>
    </aside>
  );
}

function initials(email: string | undefined) {
  if (!email) return "?";
  return email.slice(0, 2).toUpperCase();
}
