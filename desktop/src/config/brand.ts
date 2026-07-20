// Single source of truth for BRUTE's official brand identity strings and
// asset references, so brand text/colors are never scattered/hardcoded
// across individual pages. See branding/README.md for how to replace the
// underlying image assets.

import logoUrl from "../assets/brand/brute-logo.png";
import iconUrl from "../assets/brand/brute-app-icon.png";

export const brand = {
  productName: "BRUTE Runtime",
  shortName: "BRUTE",
  tagline: "POWER. CONTROL. PERFORMANCE.",
  /** Horizontal lockup (wordmark + symbol) — brand placement, never the window/exe icon. */
  logoUrl,
  /** Square symbol mark — compact in-app placements (sidebar), matching the OS app icon. */
  iconUrl,
  /** Metallic silver/chrome + red-accent identity, on a black/near-black ground. */
  colors: {
    metallicSilver: "#e8e8ea",
    metallicSilverDim: "#9a9aa0",
    accentRed: "#c62f30",
    accentRedBright: "#e6484a",
    background: "#0a0a0c",
  },
} as const;
