import { z } from "zod";

export const SchoolTypeValues = [
  "Grundschule",
  "Hauptschule",
  "Realschule",
  "Gymnasium",
  "Gesamtschule",
] as const;

export type SchoolType = (typeof SchoolTypeValues)[number];

export const StundentafelFormSchema = z.object({
  name: z.string().min(1, "Name is required"),
  grade_level: z.number().int().min(1, "Grade must be at least 1").max(13),
  school_type: z.enum(SchoolTypeValues),
});

export const EntryFormSchema = z.object({
  subject_id: z.string().min(1, "Subject is required"),
  hours_per_week: z.number().int().min(1, "Hours must be at least 1"),
  preferred_block_size: z.number().int().min(1).max(2),
});

export type StundentafelFormValues = z.infer<typeof StundentafelFormSchema>;
export type EntryFormValues = z.infer<typeof EntryFormSchema>;
