import { z } from "zod";

export const TeacherFormSchema = z.object({
  first_name: z.string().trim().min(1, "First name is required").max(100),
  last_name: z.string().trim().min(1, "Last name is required").max(100),
  short_code: z.string().trim().min(1, "Short code is required").max(10),
  max_hours_per_week: z.number().int().min(1),
  reserve_hours_per_week: z.number().int().min(0),
  working_days: z.array(z.number().int().min(0).max(4)).nullable(),
});

export type TeacherFormValues = z.infer<typeof TeacherFormSchema>;
