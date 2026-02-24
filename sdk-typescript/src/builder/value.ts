import type { SmqlValue } from "../types.js";

export function escapeString(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

export function valueToSmql(val: SmqlValue): string {
  if (val === null) return "NULL";
  if (typeof val === "boolean") return val ? "true" : "false";
  if (typeof val === "number") return String(val);
  if (typeof val === "string") return `"${escapeString(val)}"`;
  if (Array.isArray(val)) {
    const items = val.map(valueToSmql);
    return `[${items.join(", ")}]`;
  }
  // object
  const entries = Object.entries(val);
  if (entries.length === 0) return "{}";
  const fields = entries.map(([k, v]) => `${k}: ${valueToSmql(v)}`);
  return `{${fields.join(", ")}}`;
}

export function formatDataFields(data: Record<string, SmqlValue>): string {
  const entries = Object.entries(data);
  if (entries.length === 0) return "";
  const fields = entries.map(([k, v]) => `${k}: ${valueToSmql(v)}`);
  return ` ${fields.join(", ")} `;
}
