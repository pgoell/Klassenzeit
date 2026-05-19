import { createFileRoute } from "@tanstack/react-router";
import { AdminUsersPage } from "@/features/admin/admin-users-page";

export const Route = createFileRoute("/_authed/admin-users")({
  component: AdminUsersPage,
});
