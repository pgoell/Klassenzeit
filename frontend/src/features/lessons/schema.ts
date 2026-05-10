import { z } from "zod";

export const LessonFormSchema = z.object({
  school_class_ids: z.array(z.string().min(1)).min(1, "lessons.form.classesRequired"),
  subject_id: z.string().min(1, "Subject is required"),
  teacher_id: z.string().nullable(),
  hours_per_week: z.number().int().min(1, "Hours must be at least 1"),
  preferred_block_size: z.number().int().min(1).max(2),
});

export type LessonFormValues = z.infer<typeof LessonFormSchema>;
