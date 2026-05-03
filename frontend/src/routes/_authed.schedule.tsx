import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";
import { SchedulePage } from "@/features/schedule/schedule-page";

const scheduleSearchSchema = z.object({
  view: z.enum(["class", "teacher", "room"]).optional(),
  class: z.string().min(1).optional(),
  teacher: z.string().min(1).optional(),
  room: z.string().min(1).optional(),
});

export const Route = createFileRoute("/_authed/schedule")({
  component: SchedulePage,
  validateSearch: scheduleSearchSchema,
});
