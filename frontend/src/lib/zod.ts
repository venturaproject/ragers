import { z } from "zod"

const _emptyStringToUndefined = z.literal("").transform(() => undefined)

export const asOptional = <T>(schema: z.ZodType<T>) =>
  schema.optional().or(_emptyStringToUndefined)

export const zodErrorMap: z.ZodErrorMap = (issue) => {
  const requiredMessage = "This field is required."
  let message = issue.message ?? "Invalid input"

  if (issue.code === "invalid_type" && issue.input == null) {
    message = requiredMessage
  } else if (issue.code === "invalid_format" && issue.format === "email") {
    message = "Please input a valid email address."
  } else if (issue.code === "invalid_format" && issue.format === "url") {
    message = "Please input a valid URL."
  } else if (issue.code === "too_small" && issue.minimum === 1 && !issue.input) {
    message = requiredMessage
  }

  return { message: message.endsWith(".") ? message : `${message}.` }
}

export { z }
