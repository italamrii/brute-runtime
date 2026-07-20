import type { CSSProperties, ElementType, ReactNode } from "react";

/**
 * Wraps an always-LTR technical string (a Windows path, a SHA-256 hash, a
 * profile/conversation ID, an enum value like `local_unverified_source`,
 * a model architecture/quantization code) so it renders in the correct
 * visual order inside an Arabic/RTL page instead of being reordered by
 * the browser's bidi algorithm (e.g. a `\\?\`-prefixed path's backslashes
 * floating to the wrong end of the string).
 *
 * `dir="ltr"` plus `unicode-bidi: isolate` only change how the browser
 * *renders* the text - unlike inserting Unicode bidi control characters
 * into the string, they add nothing to the DOM text content, so selecting
 * and copying the value always yields the exact original string.
 */
export function TechnicalValue({
  children,
  className,
  title,
  as = "span",
  style,
}: {
  children: ReactNode;
  className?: string;
  title?: string;
  as?: ElementType;
  style?: CSSProperties;
}) {
  const Component = as;
  return (
    <Component dir="ltr" className={className} title={title} style={{ unicodeBidi: "isolate", ...style }}>
      {children}
    </Component>
  );
}
