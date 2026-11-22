export default function ServiceSwitch({ label, running, disabled, onChange }: {
  label: string;
  running: boolean;
  disabled: boolean;
  onChange: (enabled: boolean) => void;
}) {
  return (
    <label className="service-switch">
      <input
        type="checkbox"
        role="switch"
        checked={running}
        disabled={disabled}
        onChange={event => onChange(event.target.checked)}
      />
      {label}
    </label>
  );
}
