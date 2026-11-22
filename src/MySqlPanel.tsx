import { invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import ServiceSwitch from "./ServiceSwitch";

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
  const message = busy ? null : error ?? status?.error;
  const label = busy === "start" ? "Starting…"
    : busy === "stop" ? "Stopping…"
    : status?.ready ? "Running"
    : running ? "Not ready"
    : message ? "Error"
    : !desktop ? "Unavailable"
    : !status ? "Checking…"
    : "Stopped";
  const tone = busy ? "busy" : status?.ready ? "ready" : running || message ? "warning" : "neutral";

  return (
    <>
      <ServiceSwitch
        label="MySQL server"
        status={label}
        tone={tone}
        running={running}
        disabled={!desktop || !status || !!busy || (!running && !installed)}
        onChange={enabled => void act(enabled ? "start" : "stop")}
      />
      {message && <p className="notice error" role="alert">{message}</p>}
      {password && (
        <details className="disclosure credential">
          <summary>Temporary root password</summary>
          <div className="disclosure-body">
            <label>Temporary password
              <input className="mono" value={password.value} readOnly autoComplete="off" spellCheck={false} onFocus={event => event.target.select()} />
            </label>
            <dl className="metadata"><div><dt>Data folder</dt><dd><code>{password.dataDir}</code></dd></div></dl>
            <p className="note">Copy before closing Volt. Change it on first login.</p>
            <div className="actions"><button type="button" onClick={() => setPassword(null)}>Dismiss</button></div>
          </div>
        </details>
      )}
      {installed && status?.dataDir && (
        <details className="disclosure">
          <summary>Data folder</summary>
          <div className="disclosure-body"><code className="data-path">{status.dataDir}</code></div>
        </details>
      )}
      {status?.logs && <details className="disclosure" open={!!status.error}>
        <summary>MySQL logs</summary>
        <div className="disclosure-body"><pre>{status.logs}</pre></div>
      </details>}
    </>
  );
}
