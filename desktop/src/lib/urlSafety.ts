// Every "open in browser" action in BRUTE hands a URL to the OS's real
// default browser (via `@tauri-apps/plugin-opener`'s `openUrl`), which runs
// as a separate process and never touches the BRUTE WebView. That said,
// nothing upstream of this module guarantees the URL is a real, external,
// well-formed http(s) address - it could be a stray localhost reference left
// over from development, a malformed catalog entry, or (if a catalog was
// ever sourced from somewhere less trusted) something actively hostile like
// a `javascript:` or `file://` URL. This module is the single choke point
// every such URL must pass through before it is ever opened, so the check
// only has to be written once and cannot be silently skipped by a new call
// site. See docs/security-model.md.

export interface UrlSafetyResult {
  safe: boolean;
  hostname: string | null;
  reason: string | null;
}

const ALLOWED_SCHEMES = new Set(["http:", "https:"]);

const DISALLOWED_EXACT_HOSTS = new Set(["localhost", "127.0.0.1", "0.0.0.0", "::1", "[::1]"]);

function isLoopbackOrPrivateHost(hostname: string): boolean {
  const host = hostname.toLowerCase();
  if (DISALLOWED_EXACT_HOSTS.has(host)) return true;
  if (host.endsWith(".localhost")) return true;
  if (host === "0") return true;

  // IPv4 loopback (127.0.0.0/8), link-local (169.254.0.0/16), and the
  // private ranges (RFC 1918) - none of these can be a legitimate public
  // "official source" for a model, and every one of them is exactly the
  // shape of address a stray dev-server reference would produce.
  const ipv4 = host.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
  if (ipv4) {
    const [a, b] = [Number(ipv4[1]), Number(ipv4[2])];
    if (a === 127) return true;
    if (a === 169 && b === 254) return true;
    if (a === 10) return true;
    if (a === 172 && b >= 16 && b <= 31) return true;
    if (a === 192 && b === 168) return true;
    if (a === 0) return true;
    return false;
  }

  if (host.startsWith("[") || host.includes(":")) {
    // Any IPv6 literal, loopback or not, is not a link BRUTE's catalog is
    // expected to produce - reject rather than try to parse ranges.
    return true;
  }

  return false;
}

/**
 * Validate a URL before it is ever handed to the OS browser. Returns
 * `safe: false` for anything malformed, non-http(s), or pointing at a
 * loopback/private/local address - callers must not call `openUrl` unless
 * `safe` is `true`.
 */
export function checkUrlSafety(rawUrl: string): UrlSafetyResult {
  if (!rawUrl || typeof rawUrl !== "string") {
    return { safe: false, hostname: null, reason: "url_empty" };
  }

  let parsed: URL;
  try {
    parsed = new URL(rawUrl);
  } catch {
    return { safe: false, hostname: null, reason: "url_malformed" };
  }

  if (!ALLOWED_SCHEMES.has(parsed.protocol)) {
    return { safe: false, hostname: parsed.hostname || null, reason: "url_scheme_not_allowed" };
  }

  if (!parsed.hostname) {
    return { safe: false, hostname: null, reason: "url_missing_host" };
  }

  if (isLoopbackOrPrivateHost(parsed.hostname)) {
    return { safe: false, hostname: parsed.hostname, reason: "url_loopback_or_private" };
  }

  return { safe: true, hostname: parsed.hostname, reason: null };
}
