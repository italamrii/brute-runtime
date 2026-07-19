/** Compact monochrome line icons for the navigation rail and console chrome. */

import type { ReactNode } from "react";

type IconProps = { size?: number; className?: string; title?: string };

function Svg({ size = 16, className, title, children }: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.35"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden={title ? undefined : true}
      role={title ? "img" : undefined}
    >
      {title ? <title>{title}</title> : null}
      {children}
    </svg>
  );
}

export function IconOverview(props: IconProps) {
  return (
    <Svg {...props}>
      <rect x="2" y="2" width="5" height="5" rx="0.8" />
      <rect x="9" y="2" width="5" height="5" rx="0.8" />
      <rect x="2" y="9" width="5" height="5" rx="0.8" />
      <rect x="9" y="9" width="5" height="5" rx="0.8" />
    </Svg>
  );
}

export function IconHardware(props: IconProps) {
  return (
    <Svg {...props}>
      <rect x="3" y="4" width="10" height="8" rx="1" />
      <path d="M6 12.5v1.5M10 12.5v1.5M5 2.5v1.5M8 2.5v1.5M11 2.5v1.5" />
      <path d="M5.5 7h5M5.5 9.5h3.5" />
    </Svg>
  );
}

export function IconModels(props: IconProps) {
  return (
    <Svg {...props}>
      <path d="M3 4.5 8 2l5 2.5v5L8 12.5 3 10z" />
      <path d="M8 7v5.5M3 4.5 8 7l5-2.5" />
    </Svg>
  );
}

export function IconOptimize(props: IconProps) {
  return (
    <Svg {...props}>
      <circle cx="8" cy="8" r="2.2" />
      <path d="M8 2.2v1.8M8 12v1.8M2.2 8h1.8M12 8h1.8M3.8 3.8l1.3 1.3M10.9 10.9l1.3 1.3M12.2 3.8l-1.3 1.3M5.1 10.9l-1.3 1.3" />
    </Svg>
  );
}

export function IconRun(props: IconProps) {
  return (
    <Svg {...props}>
      <path d="M5 3.5 12.5 8 5 12.5z" />
    </Svg>
  );
}

export function IconProfiles(props: IconProps) {
  return (
    <Svg {...props}>
      <path d="M3 4h10M3 8h10M3 12h7" />
      <circle cx="12.2" cy="12" r="1.4" />
    </Svg>
  );
}

export function IconHealth(props: IconProps) {
  return (
    <Svg {...props}>
      <path d="M2.5 8h2.2l1.3-3.2 2.2 6.4 1.4-3.2H13.5" />
    </Svg>
  );
}

export function IconSettings(props: IconProps) {
  return (
    <Svg {...props}>
      <circle cx="8" cy="8" r="2" />
      <path d="M8 2.5v1.4M8 12.1v1.4M2.5 8h1.4M12.1 8h1.4M4.1 4.1l1 1M10.9 10.9l1 1M11.9 4.1l-1 1M5.1 10.9l-1 1" />
    </Svg>
  );
}

export function IconTrophy(props: IconProps) {
  return (
    <Svg {...props}>
      <path d="M5 3h6v3.2a3 3 0 0 1-6 0z" />
      <path d="M5 4.2H3.4A1.4 1.4 0 0 0 3.4 7H5M11 4.2h1.6A1.4 1.4 0 0 1 12.6 7H11" />
      <path d="M6.5 12.5h3M8 9.2v3.3" />
    </Svg>
  );
}

export function IconShield(props: IconProps) {
  return (
    <Svg {...props}>
      <path d="M8 2.5 12.5 4.5v3.4c0 2.6-1.9 4.5-4.5 5.6-2.6-1.1-4.5-3-4.5-5.6V4.5z" />
      <path d="M6.2 8.1 7.5 9.4 10 6.8" />
    </Svg>
  );
}
