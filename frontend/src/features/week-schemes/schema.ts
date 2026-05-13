import { z } from "zod";

export const WeekSchemeFormSchema = z.object({
  name: z.string().trim().min(1, "Name is required").max(100),
  description: z.string().trim().max(500).optional(),
});

export type WeekSchemeFormValues = z.infer<typeof WeekSchemeFormSchema>;

export const TimeBlockFormSchema = z.object({
  day_of_week: z.number().int().min(0).max(4),
  position: z.number().int().min(1),
  start_time: z.string().regex(/^[0-2][0-9]:[0-5][0-9]$/, "invalid_time"),
  end_time: z.string().regex(/^[0-2][0-9]:[0-5][0-9]$/, "invalid_time"),
  kind: z.enum(["lesson", "break"]),
});
export type TimeBlockFormValues = z.infer<typeof TimeBlockFormSchema>;
