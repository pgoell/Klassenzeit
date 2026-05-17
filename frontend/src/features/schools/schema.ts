import { z } from "zod";

export const SchoolFormSchema = z.object({
  name: z.string().trim().min(1, "Name is required").max(120),
  short_name: z.string().trim().max(20).optional(),
});

export type SchoolFormValues = z.infer<typeof SchoolFormSchema>;
