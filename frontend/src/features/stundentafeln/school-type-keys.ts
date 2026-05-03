import type { SchoolType } from "./schema";

export function schoolTypeLabelKey(
  value: SchoolType,
): `stundentafeln.fields.schoolType.${SchoolType}` {
  return `stundentafeln.fields.schoolType.${value}`;
}
