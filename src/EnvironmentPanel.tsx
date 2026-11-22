import { invoke, isTauri } from "@tauri-apps/api/core";
import { type ReactNode, useCallback, useEffect, useState } from "react";

export type RuntimeId = "php" | "apache" | "mysql";
export type ManagedFolders = Partial<Record<RuntimeId, string>>;

type Runtime = {
  id: RuntimeId;
  folder: string;
  downloads: string[];
  versions: string[];
  activeVersion: string | null;
  activeFolder: string | null;
};
type Status = {
  runtimes: Runtime[];
  pathEnabled: boolean;
  pathError: string | null;
};
type RunAction = (message: string, action: () => Promise<Status | void>) => Promise<void>;

const names: Record<RuntimeId, string> = { php: "PHP", apache: "Apache", mysql: "MySQL" };
const desktop = isTauri();
const versionLabel = (version: string) => version === "legacy" ? "Existing install" : version;

function RuntimeCard({ runtime, locked, busy, run, children }: {
  runtime: Runtime;
  locked: boolean;
  busy: boolean;
  run: RunAction;
  children?: ReactNode;
}) {
  const [source, setSource] = useState("");
  const [selectedDownload, setSelectedDownload] = useState("");
  const name = names[runtime.id];
  const available = runtime.downloads.filter(version => !runtime.versions.includes(version));
  const download = available.includes(selectedDownload) ? selectedDownload : available[0];
  const active = runtime.activeVersion;

  return (
    <section className="runtime-card" aria-labelledby={`runtime-${runtime.id}`}>
      <div className="runtime-heading">
        <h3 id={`runtime-${runtime.id}`}>{name}</h3>
        <select
          className="active-version"
          aria-label={`Active ${name} version`}
          aria-describedby={locked ? `runtime-${runtime.id}-locked` : undefined}
          value={active ?? ""}
          disabled={busy || locked || !runtime.versions.length}
          onChange={event => {
            const version = event.target.value;
            void run(`Switching ${name}…`, () => invoke<Status>("select_runtime", { component: runtime.id, version }));
          }}
        >
          {!active && <option value="">Not installed</option>}
          {active && !runtime.versions.includes(active) && <option value={active} disabled>{versionLabel(active)} (missing)</option>}
          {runtime.versions.map(version => <option key={version} value={version}>{versionLabel(version)}</option>)}
        </select>
      </div>
      {locked && <span className="sr-only" id={`runtime-${runtime.id}-locked`}>Stop {runtime.id === "mysql" ? "MySQL" : "Apache"} to switch versions.</span>}
      {children}
      <details>
        <summary>Install / import</summary>
        {download && (
          <div className="actions download-actions">
            <select aria-label={`${name} version to download`} value={download} disabled={busy} onChange={event => setSelectedDownload(event.target.value)}>
              {available.map(version => <option key={version}>{version}</option>)}
            </select>
            <button type="button" disabled={busy} onClick={() => void run(
              `Downloading ${name} ${download}…`,
              () => invoke<Status>("install_runtime", { component: runtime.id, version: download, source: null }),
            )}>Download ZIP</button>
          </div>
        )}
        <form onSubmit={event => {
          event.preventDefault();
          if (busy || !source.trim()) return;
          void run(`Importing ${name}…`, async () => {
            const next = await invoke<Status>("install_runtime", { component: runtime.id, version: null, source });
            setSource("");
            return next;
          });
        }}>
          <label>
            Extracted {name} folder
            <input spellCheck={false} disabled={busy} value={source} onChange={event => setSource(event.target.value)} />
          </label>
          <div className="actions"><button type="submit" disabled={busy || !source.trim()}>Import Folder</button></div>
        </form>
        <p className="note data-path">{runtime.activeFolder ?? runtime.folder}</p>
      </details>
    </section>
  );
}

export default function EnvironmentPanel({ onChange, apacheRunning, mysqlRunning, apacheControls, mysqlControls }: {
  onChange: (folders: ManagedFolders) => void;
  apacheRunning: boolean;
  mysqlRunning: boolean;
  apacheControls: ReactNode;
  mysqlControls: ReactNode;
}) {
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const update = useCallback((next: Status) => {
    setStatus(next);
    const folders: ManagedFolders = {};
    for (const runtime of next.runtimes) if (runtime.activeFolder) folders[runtime.id] = runtime.activeFolder;
    onChange(folders);
  }, [onChange]);

  useEffect(() => {
    if (!desktop) return;
    let active = true;
    void invoke<Status>("environment_status").then(next => { if (active) update(next); })
      .catch(error => { if (active) setError(String(error)); });
    return () => { active = false; };
  }, [update]);

  const run: RunAction = async (message, action) => {
    setBusy(message);
    setError(null);
    try {
      const next = await action();
      if (next) update(next);
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section aria-labelledby="environment-heading">
      <div className="section-heading">
        <h2 id="environment-heading">Environment</h2>
        <div className="actions">
          <button type="button" disabled={!desktop || !!busy} onClick={() => void run("Opening runtimes…", () => invoke<void>("open_runtime_folder"))}>Open Runtimes</button>
          <button type="button" disabled={!desktop || !!busy} onClick={() => void run("Scanning runtimes…", () => invoke<Status>("environment_status"))}>Rescan</button>
        </div>
      </div>
      {busy && <p className="status" role="status">{busy}</p>}
      {(error ?? status?.pathError) && <p className="error" role="alert">{error ?? status?.pathError}</p>}
      {!desktop && <p className="note">Open Volt Desktop to manage runtimes.</p>}
      {desktop && !status && !error && <p className="status" role="status">Loading runtimes…</p>}
      {status?.runtimes.map(runtime => (
        <RuntimeCard
          key={runtime.id}
          runtime={runtime}
          locked={runtime.id === "mysql" ? mysqlRunning : apacheRunning}
          busy={!!busy}
          run={run}
        >
          {runtime.id === "apache" ? apacheControls : runtime.id === "mysql" ? mysqlControls : null}
        </RuntimeCard>
      ))}
      {status && <div className="path-settings">
        <label className="path-toggle">
          <input type="checkbox" checked={status.pathEnabled} disabled={!!busy} onChange={event => {
            const enabled = event.target.checked;
            void run("Updating user PATH…", () => invoke<Status>("set_environment_path", { enabled }));
          }} />
          Add active versions to user PATH
        </label>
        <span className="note">Applies to new terminals.</span>
      </div>}
    </section>
  );
}
