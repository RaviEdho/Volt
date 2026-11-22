import { invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

type Status = {
  running: boolean;
  ready: boolean;
  error: string | null;
  logs: string;
  dataDir: string;
};

type Started = { status: Status; temporaryPassword: string | null };
const desktop = isTauri();

export default function MySqlPanel({ installed, onRunningChange }: {
  installed: boolean;
  onRunningChange: (running: boolean) => void;
}) {
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState<"start" | "stop" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [password, setPassword] = useState<{ value: string; dataDir: string } | null>(null);

  useEffect(() => {
    if (!desktop || busy) return;
    let active = true;
    let checking = false;
    async function refresh() {
      if (checking) return;
      checking = true;
      try {
        const next = await invoke<Status>("mysql_status");
        if (active) setStatus(next);
      } catch (error) {
        if (active) setError(String(error));
      } finally {
        checking = false;
      }
    }
    void refresh();
    const timer = window.setInterval(refresh, 2000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [busy]);

  async function act(action: "start" | "stop") {
    setBusy(action);
    setError(null);
    try {
      if (action === "start") {
        const next = await invoke<Started>("start_mysql");
        setStatus(next.status);
        if (next.temporaryPassword) setPassword({ value: next.temporaryPassword, dataDir: next.status.dataDir });
        else if (next.status.ready) setPassword(previous => previous?.dataDir === next.status.dataDir ? previous : null);
      } else {
        setStatus(await invoke<Status>("stop_mysql"));
      }
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(null);
    }
  }

  const running = status?.running ?? false;
  useEffect(() => { onRunningChange(running || !!busy); }, [busy, running, onRunningChange]);
  const label = busy === "start" ? "Starting MySQL (first start initializes the data folder)…"
    : busy === "stop" ? "Stopping MySQL…"
    : status?.ready ? "MySQL is ready at 127.0.0.1:3307"
    : running ? "MySQL is running but not ready"
    : !installed ? "Select an installed MySQL version to enable it."
    : "MySQL stopped";
  const message = busy ? null : error ?? status?.error;

  return (
    <>
      <div className="service-actions">
        <label className="service-switch">
          <input type="checkbox" role="switch" checked={running} disabled={!desktop || !status || !!busy || (!running && !installed)} onChange={event => void act(event.target.checked ? "start" : "stop")} />
          Enable MySQL
        </label>
      </div>
      <p className="status" role="status">{label}</p>
      {installed && status?.dataDir && <p className="note data-path">Data: {status.dataDir}</p>}
      {message && <p className="error" role="alert">{message}</p>}
      {password && (
        <details>
          <summary>Show temporary MySQL root password</summary>
          <p className="note data-path">For: {password.dataDir}</p>
          <p className="note">MySQL requires you to change this on first login. Copy it before closing Volt; Volt does not save it. See README.md for the commands.</p>
          <label className="password-field">Temporary password
            <input value={password.value} readOnly autoComplete="off" spellCheck={false} onFocus={event => event.target.select()} />
          </label>
          <button type="button" onClick={() => setPassword(null)}>Dismiss password</button>
        </details>
      )}
      {status?.logs && <details open={!!status.error}>
        <summary>MySQL logs</summary>
        <pre>{status.logs}</pre>
      </details>}
    </>
  );
}
