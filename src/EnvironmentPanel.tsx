import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { type ReactNode, useCallback, useEffect, useState } from "react";
import Icon from "./Icon";

export type RuntimeId = "php" | "apache" | "mysql";
export type ManagedFolders = Partial<Record<RuntimeId, string>>;

type Runtime = {
  id: RuntimeId;
  folder: string;
  downloads: string[];
  versions: string[];
  activeVersion: string | null;
  activeFolder: string | null;
  composerInstalled: boolean;
};
type Status = {
  runtimes: Runtime[];
  pathEnabled: boolean;
  pathError: string | null;
};
type RunAction = (message: string, action: () => Promise<Status | void>) => Promise<void>;

const runtimeIds: RuntimeId[] = ["apache", "php", "mysql"];
const runtimeInfo = {
  apache: { name: "Apache", icon: "server", endpoint: "127.0.0.1:8080" },
  php: { name: "PHP", icon: "code", endpoint: null },
  mysql: { name: "MySQL", icon: "database", endpoint: "127.0.0.1:3307" },
} as const;
const desktop = isTauri();
const versionLabel = (version: string) => version === "legacy" ? "Existing install" : version;

function RuntimeCard({ id, runtime, locked, busy, run, action, children }: {
  id: RuntimeId;
  runtime?: Runtime;
  locked: boolean;
  busy: boolean;
  run: RunAction;
  action?: ReactNode;
  children?: ReactNode;
}) {
  const [selectedDownload, setSelectedDownload] = useState("");
  const { name, icon, endpoint } = runtimeInfo[id];
  const available = runtime?.downloads.filter(version => !runtime.versions.includes(version)) ?? [];
  const download = available.includes(selectedDownload) ? selectedDownload : available[0];
  const active = runtime?.activeVersion;

  const importRuntime = (directory: boolean) => run(`Importing ${name}…`, async () => {
    const source = await open(directory
      ? { title: `Select the extracted ${name} folder`, directory: true }
      : { title: `Select a ${name} ZIP`, filters: [{ name: "ZIP archives", extensions: ["zip"] }] });
    if (source === null) return;
    return invoke<Status>("install_runtime", { component: id, version: null, source });
  });

  return (
    <section className="runtime-card" aria-labelledby={`runtime-${id}`}>
      <div className="runtime-identity">
        <span className="runtime-icon"><Icon name={icon} /></span>
        <div>
          <h3 id={`runtime-${id}`}>{name}</h3>
          {endpoint ? <code className="endpoint">{endpoint}</code> : <span className="muted">{runtime?.composerInstalled ? "Apache module · Composer installed" : "Apache module"}</span>}
        </div>
      </div>
      <label className="runtime-version" title={locked ? `Stop ${id === "mysql" ? "MySQL" : "Apache"} to switch versions.` : undefined}>
        <span>Active version</span>
        <select
          className="version-select"
          aria-label={`Active ${name} version`}
          aria-describedby={locked ? `runtime-${id}-locked` : undefined}
          value={active ?? ""}
          disabled={busy || locked || !runtime?.versions.length}
          onChange={event => {
            const version = event.target.value;
            void run(`Switching ${name}…`, () => invoke<Status>("select_runtime", { component: id, version }));
          }}
        >
          {!active && <option value="">{runtime ? "Not installed" : "—"}</option>}
          {active && !runtime?.versions.includes(active) && <option value={active} disabled>{versionLabel(active)} (missing)</option>}
          {runtime?.versions.map(version => <option key={version} value={version}>{versionLabel(version)}</option>)}
        </select>
      </label>
      {locked && <span className="sr-only" id={`runtime-${id}-locked`}>Stop {id === "mysql" ? "MySQL" : "Apache"} to switch versions.</span>}
      {children}
      {runtime && (
        <div className="runtime-tools">
          <details className="disclosure runtime-install">
            <summary>Install / import</summary>
            <div className="disclosure-body">
              {download && (
                <div className="actions">
                  <select className="version-select" aria-label={`${name} version to download`} value={download} disabled={busy} onChange={event => setSelectedDownload(event.target.value)}>
                    {available.map(version => <option key={version}>{version}</option>)}
                  </select>
                  <button type="button" disabled={busy} onClick={() => void run(
                    `Downloading ${name} ${download}…`,
                    () => invoke<Status>("install_runtime", { component: id, version: download, source: null }),
                  )}>Download ZIP</button>
                </div>
              )}
              <div className="actions">
                <button type="button" disabled={busy} onClick={() => void importRuntime(false)}>Import ZIP…</button>
                <button type="button" disabled={busy} onClick={() => void importRuntime(true)}>Import folder…</button>
              </div>
              {id === "php" && (
                <>
                  <p className="note">PHP installs include Composer. PHP ZIP and folder imports also download Composer.</p>
                  {runtime.activeFolder && !runtime.composerInstalled && (
                    <div className="actions">
                      <button type="button" disabled={busy} onClick={() => void run(
                        "Installing Composer…",
                        () => invoke<Status>("install_php_composer"),
                      )}>Install Composer for active PHP</button>
                    </div>
                  )}
                </>
              )}
              <p className="note">Only import binaries you trust. Volt runs them to check the version.</p>
              <dl className="metadata"><div>
                <dt>{runtime.activeFolder ? "Install path" : "Runtime folder"}</dt>
                <dd><code>{runtime.activeFolder ?? runtime.folder}</code></dd>
              </div></dl>
            </div>
          </details>
          {action}
        </div>
      )}
    </section>
  );
}

export default function EnvironmentPanel({ onChange, apacheRunning, mysqlRunning, apacheControls, mysqlControls, apacheAction }: {
  onChange: (folders: ManagedFolders) => void;
  apacheRunning: boolean;
  mysqlRunning: boolean;
  apacheControls: ReactNode;
  mysqlControls: ReactNode;
  apacheAction: ReactNode;
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
          <button type="button" disabled={!desktop || !!busy} onClick={() => void run("Opening runtimes…", () => invoke<void>("open_runtime_folder"))}>
            <Icon name="folder" /> Open runtimes
          </button>
          <button type="button" disabled={!desktop || !!busy} onClick={() => void run("Scanning runtimes…", () => invoke<Status>("environment_status"))}>
            <Icon name="refresh" /> Rescan
          </button>
        </div>
      </div>
      {(busy || (desktop && !status && !error)) && <p className="notice" role="status">{busy ?? "Loading runtimes…"}</p>}
      {(error ?? status?.pathError) && <p className="notice error" role="alert">{error ?? status?.pathError}</p>}
      <div className="panel">
        {runtimeIds.map(id => (
          <RuntimeCard
            key={id}
            id={id}
            runtime={status?.runtimes.find(runtime => runtime.id === id)}
            locked={id === "mysql" ? mysqlRunning : apacheRunning}
            busy={!!busy}
            run={run}
            action={id === "apache" ? apacheAction : null}
          >
            {id === "apache" ? apacheControls : id === "mysql" ? mysqlControls : null}
          </RuntimeCard>
        ))}
        {status && <div className="path-settings">
          <label className="path-toggle">
            <input type="checkbox" checked={status.pathEnabled} disabled={!!busy} aria-describedby="path-hint" onChange={event => {
              const enabled = event.target.checked;
              void run("Updating user PATH…", () => invoke<Status>("set_environment_path", { enabled }));
            }} />
            Add active versions to user PATH
          </label>
          <span className="note" id="path-hint">New terminals only</span>
        </div>}
      </div>
    </section>
  );
}
