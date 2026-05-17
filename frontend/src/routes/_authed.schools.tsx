import { createFileRoute } from "@tanstack/react-router";
import { SchoolsPage } from "@/features/schools/schools-page";

export const Route = createFileRoute("/_authed/schools")({
  component: SchoolsPage,
});
