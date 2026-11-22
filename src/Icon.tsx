import type { ReactNode } from "react";

type IconName = "bolt" | "server" | "code" | "database" | "folder" | "refresh" | "external";

const shapes: Record<IconName, ReactNode> = {
  bolt: <path d="m13 2-8 12h6l-1 8 9-12h-6l1-8Z" fill="currentColor" stroke="none" />,
  server: <><rect x="3" y="4" width="18" height="7" rx="1.5" /><rect x="3" y="13" width="18" height="7" rx="1.5" /><path d="M7 7.5h.01M7 16.5h.01M12 7.5h5m-5 9h5" /></>,
  code: <path d="m8 7-5 5 5 5m8-10 5 5-5 5m-3-13-2 16" />,
  database: <><ellipse cx="12" cy="5" rx="8" ry="3" /><path d="M4 5v14a8 3 0 0 0 16 0V5M4 12a8 3 0 0 0 16 0" /></>,
  folder: <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V7Z" />,
  refresh: <path d="M20 11a8 8 0 1 0-2 6m2-12v6h-6" />,
  external: <path d="M13 5h6v6m0-6-8 8m5 0v5a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1h5" />,
};

export default function Icon({ name }: { name: IconName }) {
  return (
    <svg className="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {shapes[name]}
    </svg>
  );
}
