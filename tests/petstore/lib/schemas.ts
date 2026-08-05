// The shapes the pet store's API speaks, declared once.
//
// A zod schema is a runtime value, not a type declaration, so nothing that reads
// declarations finds it. `fr translate` reads the builder chain instead — which is
// what lets a Pydantic model come out the other side with the same fields.
import * as z from "zod"

export const petCreateSchema = z.object({
  name: z.string().min(1).max(64),
  species: z.string(),
  ageMonths: z.number().int(),
  tags: z.array(z.string()),
  microchipId: z.string().optional(),
  arrivedAt: z.date(),
})

export const petPatchSchema = z.object({
  name: z.string().optional(),
  ageMonths: z.number().int().optional(),
  tags: z.array(z.string()).optional(),
})

export const statusUpdateSchema = z.object({
  status: z.string(),
  note: z.string().nullable(),
})

export const petSearchSchema = z.object({
  species: z.string().optional(),
  minAgeMonths: z.number().int().optional(),
  maxAgeMonths: z.number().int().optional(),
  tags: z.array(z.string()),
})

export const photoCreateSchema = z.object({
  url: z.string(),
  caption: z.string().optional(),
  widthPx: z.number().int(),
  heightPx: z.number().int(),
})
