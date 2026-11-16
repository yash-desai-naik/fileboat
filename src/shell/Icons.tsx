import { useId, type ReactNode } from "react";

// Fileboat's own 16 px outline icons (1.5 px stroke), drawn like Tableboat's. Window caption
// glyphs are 10 px with 1 px strokes, drawn on whole pixels so they stay crisp at any scaling.

type IconProps = { size?: number };

function Svg({ size = 16, children }: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export function PanelLeftIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="2" y="3" width="12" height="10" rx="1.5" />
      <path d="M6 3v10" />
    </Svg>
  );
}

export function PanelRightIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="2" y="3" width="12" height="10" rx="1.5" />
      <path d="M10 3v10" />
    </Svg>
  );
}

export function CommandIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="2" y="3" width="12" height="10" rx="1.5" />
      <path d="M5 7l2 1.5L5 10M8.5 10.5H11" />
    </Svg>
  );
}

export function FolderIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.6l1.4 1.5h5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
    </Svg>
  );
}

export function FileIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M4.5 2h4.5l3 3v7.5a1.5 1.5 0 0 1-1.5 1.5h-6A1.5 1.5 0 0 1 3 12.5v-9A1.5 1.5 0 0 1 4.5 2zM9 2v3h3" />
    </Svg>
  );
}

/** An eye: the preview pane. */
export function PreviewIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z" />
      <circle cx="8" cy="8" r="2" />
    </Svg>
  );
}

export function CloseSmallIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />
    </Svg>
  );
}

export function SearchIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="7" cy="7" r="4.5" />
      <path d="m10.5 10.5 3 3" />
    </Svg>
  );
}

export function ChevronIcon({ open, size = 12 }: { open: boolean; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 12 12"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      style={{ transform: open ? "rotate(90deg)" : undefined }}
    >
      <path d="M4.5 3l3 3-3 3" />
    </svg>
  );
}

function Caption({ children }: { children: ReactNode }) {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth={1} aria-hidden="true">
      {children}
    </svg>
  );
}

export const MinimizeGlyph = () => (
  <Caption>
    <path d="M0 5.5h10" />
  </Caption>
);

export const MaximizeGlyph = () => (
  <Caption>
    <rect x="0.5" y="0.5" width="9" height="9" rx="1" />
  </Caption>
);

export const RestoreGlyph = () => (
  <Caption>
    <rect x="0.5" y="2.5" width="7" height="7" rx="1" />
    <path d="M2.5 2.5V1.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" />
  </Caption>
);

export const CloseGlyph = () => (
  <Caption>
    <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" />
  </Caption>
);

/** The app mark (same drawing as src-tauri/app-icon.svg): a boat whose sail is a folder. */
export function AppMark({ size = 16 }: IconProps) {
  const id = useId();
  return (
    <svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true">
      <defs>
        <linearGradient id={`${id}front`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#6fb4f4" />
          <stop offset="1" stopColor="#2f7fd3" />
        </linearGradient>
        <linearGradient id={`${id}hull`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#2a63a6" />
          <stop offset="1" stopColor="#173d6b" />
        </linearGradient>
      </defs>
      <rect x="176" y="24" width="48" height="712" rx="24" fill="#245a9b" />
      <path d="M250 94 Q250 56 288 56 H452 Q476 56 492 74 L530 118 H860 Q900 118 900 158 V660 H250 Z" fill="#245a9b" />
      <path
        d="M250 236 Q250 196 290 196 H962 Q1004 196 996 238 L918 640 Q912 664 888 664 H250 Z"
        fill={`url(#${id}front)`}
      />
      <path d="M22 700 H1002 L886 946 Q866 990 818 990 H206 Q158 990 138 946 Z" fill={`url(#${id}hull)`} />
    </svg>
  );
}
