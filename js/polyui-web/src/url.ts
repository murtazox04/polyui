const controlCharacters = /[\u0000- \u007f-\u009f]/;

export function safeUrl(raw: unknown, allowHttp = false): string | null {
  if (typeof raw !== "string" || raw.length === 0 || raw.length > 4096) return null;
  if (controlCharacters.test(raw)) return null;
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    return null;
  }
  const allowed = parsed.protocol === "https:" || (allowHttp && parsed.protocol === "http:");
  if (!allowed || parsed.username !== "" || parsed.password !== "" || parsed.hostname === "") return null;
  return parsed.href;
}
