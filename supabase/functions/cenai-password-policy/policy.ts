export function validatePassword(password: string): string | null {
  const length = Array.from(password).length;
  if (length < 12) return "Use at least 12 characters.";
  if (new TextEncoder().encode(password).length > 72) return "Use a password no longer than 72 UTF-8 bytes.";
  if (!password.trim() || /[\u0000-\u001f\u007f]/.test(password)) return "Choose a password containing printable characters.";
  if (/^(.)\1{11,}$/su.test(password)) return "Avoid passwords made from one repeated character.";
  return null;
}

export async function isBreached(password: string, fetcher: typeof fetch = fetch): Promise<boolean> {
  const digest = await crypto.subtle.digest("SHA-1", new TextEncoder().encode(password));
  const hash = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, "0")).join("").toUpperCase();
  const prefix = hash.slice(0, 5);
  const suffix = hash.slice(5);
  // HIBP k-anonymity: send only the five-character prefix, never the password
  // or full digest. Padding reduces the information exposed by response size.
  const response = await fetcher("https://api.pwnedpasswords.com/range/" + prefix, {
    headers: { "Add-Padding": "true", "User-Agent": "CenAI-Password-Policy/1.0" },
    signal: AbortSignal.timeout(8000),
  });
  if (!response.ok) throw new Error("BREACH_CHECK_UNAVAILABLE");
  const rows = await response.text();
  for (const line of rows.split("\n")) {
    const [candidate, countText] = line.trim().split(":");
    if (candidate?.toUpperCase() === suffix && Number(countText) > 0) return true;
  }
  return false;
}
