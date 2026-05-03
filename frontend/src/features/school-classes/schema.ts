import { z } from "zod";

export const SchoolClassFormSchema = z.object({
  name: z.string().trim().min(1, "Name is required").max(100),
  grade_level: z.number().int().min(1, "Grade is required").max(13, "Grade cannot exceed 13"),
  stundentafel_id: z.string().min(1, "Curriculum is required"),
  week_scheme_id: z.string().min(1, "Week scheme is required"),
  home_room_id: z.string().nullable(),
});

export type SchoolClassFormValues = z.infer<typeof SchoolClassFormSchema>;
