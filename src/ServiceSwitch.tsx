import { useId } from "react";

export default function ServiceSwitch({ label, status, tone, running, disabled, onChange }: {
  label: string;
  status: string;
  tone: "neutral" | "ready" | "warning" | "busy";
  running: boolean;
  disabled: boolean;
  onChange: (enabled: boolean) => void;
}) {
  const statusId = useId();

  return (
    <div className="service-switch">
      <span className="service-status" data-tone={tone} id={statusId} role="status">{status}</span>
      <input
        type="checkbox"
        role="switch"
        aria-label={label}
        aria-describedby={statusId}
        checked={running}
        disabled={disabled}
        onChange={event => onChange(event.target.checked)}
      />
    </div>
  );
}
