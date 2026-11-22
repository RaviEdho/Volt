import { invoke, isTauri } from "@tauri-apps/api/core";
import { type ReactNode, useCallback, useEffect, useState } from "react";

export type RuntimeId = "php" | "apache" | "mysql";
export type ManagedFolders = Partial<Record<RuntimeId, string>>;

type Component = {
  id: RuntimeId;
  folder: string;
  downloads: string[];
  versions: string[];
  activeVersion: string | null;
  activeFolder: string | null;
};
type Status = {
  root: string;
  runtimes: Component[];
  pathEnabled: boolean;
  pathError: string | null;
};
const names: Record<RuntimeId, string> = { php: "PHP", apache: "Apache", mysql: "MySQL" };
const desktop = isTauri();
const label = (version: string) => version === "legacy" ? "Existing install (unversioned)" : version;

export default function EnvironmentPanel({ onChange, apacheRunning, mysqlRunning, apacheControls, mysqlControls }: {
  onChange: (folders: ManagedFolders) => void;
  apacheRunning: boolean;
  mysqlRunning: boolean;
  apacheControls: ReactNode;
  mysqlControls: ReactNode;
}) {
  const [status, setStatus] = useState<Status | null>(null);
  const [sources, setSources] = useState<ManagedFolders>({});
  const [downloads, setDownloads] = useState<ManagedFolders>({});
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

  async function run(message: string, action: () => Promise<Status | void>) {
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
  }

  return (
    <section aria-labelledby="environment-heading">
      <h2 id="environment-heading">Environment</h2>
      <p className="note">Choose active versions below. Apache starts with the active PHP version; MySQL runs independently. Turn a service off before switching its version.</p>
      {status && <p className="note data-path">Managed folder: {status.root}</p>}
      <div className="actions environment-actions">
        <button type="button" disabled={!desktop || !!busy} onClick={() => void run("Opening runtime folder…", () => invoke<void>("open_runtime_folder"))}>Open Runtime Folder</button>
        <button type="button" disabled={!desktop || !!busy} onClick={() => void run("Scanning runtime folders…", () => invoke<Status>("environment_status"))}>Rescan</button>
      </div>
      {status && <label className="path-toggle">
        <input type="checkbox" checked={status.pathEnabled} disabled={!!busy} onChange={event => {
          const enabled = event.target.checked;
          void run("Updating Windows user PATH…", () => invoke<Status>("set_environment_path", { enabled }));
        }} />
        Add active PHP, Apache, and MySQL versions to my Windows user PATH
      </label>}
      <p className="note">Reopen terminals after changing the active versions or PATH. Older builds are for local compatibility testing.</p>
      {busy && <p className="status" role="status">{busy}</p>}
      {(error ?? status?.pathError) && <p className="error" role="alert">{error ?? status?.pathError}</p>}
      {status?.runtimes.map(runtime => {
        const available = runtime.downloads.filter(version => !runtime.versions.includes(version));
        const download = available.find(version => version === downloads[runtime.id]) ?? available[0] ?? "";
        const missing = runtime.activeVersion && !runtime.versions.includes(runtime.activeVersion);
        return (
          <section key={runtime.id} className="runtime-card" aria-labelledby={`runtime-${runtime.id}`}>
            <h3 id={`runtime-${runtime.id}`}>{names[runtime.id]} — {runtime.activeVersion ? `${label(runtime.activeVersion)}${missing ? " (missing)" : ""}` : "Not installed"}</h3>
            {runtime.id === "apache" ? apacheControls : runtime.id === "mysql" ? mysqlControls : <p className="note">PHP runs inside Apache.</p>}
            <details>
            <summary>Manage {names[runtime.id]} versions</summary>
            <p className="note data-path">{runtime.activeFolder ?? runtime.folder}</p>
            <label className="environment-actions">
              Active {names[runtime.id]} version
              <select value={runtime.activeVersion ?? ""} disabled={!!busy || !runtime.versions.length || (runtime.id === "mysql" ? mysqlRunning : apacheRunning)} onChange={event => {
                const version = event.target.value;
                void run(`Switching ${names[runtime.id]}…`, () => invoke<Status>("select_runtime", { component: runtime.id, version }));
              }}>
                {!runtime.activeVersion && <option value="">No version installed</option>}
                {missing && <option value={runtime.activeVersion!} disabled>{label(runtime.activeVersion!)} (missing)</option>}
                {runtime.versions.map(version => <option key={version} value={version}>{label(version)}</option>)}
              </select>
            </label>
            {runtime.id === "mysql" && <p className="note">Each version uses a separate data folder. Switch back to see the databases for an earlier version.</p>}
            {available.length > 0 && <div className="actions environment-actions">
              <label>
                {names[runtime.id]} version to download
                <select value={download} disabled={!!busy} onChange={event => setDownloads({ ...downloads, [runtime.id]: event.target.value })}>
                  {available.map(version => <option key={version}>{version}</option>)}
                </select>
              </label>
              <button type="button" disabled={!!busy} onClick={() => void run(
                `Downloading and verifying ${names[runtime.id]} ${download}…`,
                () => invoke<Status>("install_runtime", { component: runtime.id, version: download, source: null }),
              )}>Download ZIP</button>
            </div>}
            <form onSubmit={event => {
              event.preventDefault();
              if (busy || !sources[runtime.id]?.trim()) return;
              void run(`Importing ${names[runtime.id]}…`, async () => {
                const next = await invoke<Status>("install_runtime", { component: runtime.id, version: null, source: sources[runtime.id] });
                setSources(previous => ({ ...previous, [runtime.id]: "" }));
                return next;
              });
            }}>
              <label>
                Or paste an extracted {names[runtime.id]} folder path
                <input spellCheck={false} disabled={!!busy} value={sources[runtime.id] ?? ""} onChange={event => setSources({ ...sources, [runtime.id]: event.target.value })} />
              </label>
              <div className="actions"><button type="submit" disabled={!!busy || !sources[runtime.id]?.trim()}>Import Folder</button></div>
            </form>
            </details>
          </section>
        );
      })}
    </section>
  );
}
