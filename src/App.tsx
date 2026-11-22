import { invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import EnvironmentPanel, { type ManagedFolders } from "./EnvironmentPanel";
import Icon from "./Icon";
import MySqlPanel from "./MySqlPanel";
import ServiceSwitch from "./ServiceSwitch";
import "./App.css";

type ApacheStatus = {
  running: boolean;
  ready: boolean;
  error: string | null;
  logs: string;
};

type Project = { slug: string; folder: string; webRoot: string };
type Action = "start_apache" | "stop_apache" | "open_demo" | "open_project" | "sync_project_hosts" | "validate_projects";
const desktop = isTauri();
const projectKey = "volt.projects";
const blankProject: Project = { slug: "", folder: "", webRoot: "public" };

function loadProjects(): Project[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(projectKey) ?? "[]");
    if (Array.isArray(saved)) {
      return saved.filter(project => project && typeof project.slug === "string"
        && typeof project.folder === "string" && typeof project.webRoot === "string");
    }
  } catch {
    // Ignore an unreadable project list.
  }
  return [];
}

function App() {
  const [status, setStatus] = useState<ApacheStatus | null>(null);
  const [pending, setPending] = useState<Action | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [projects, setProjects] = useState(loadProjects);
  const [draft, setDraft] = useState(blankProject);
  const [projectMessage, setProjectMessage] = useState<string | null>(null);
  const [projectError, setProjectError] = useState<string | null>(null);
  const [managed, setManaged] = useState<ManagedFolders>({});
  const [mysqlRunning, setMysqlRunning] = useState(false);
  const apacheInstalled = !!managed.apache && !!managed.php;

  useEffect(() => {
    if (!desktop || pending) return;
    let active = true;
    let checking = false;
    async function refresh() {
      if (checking) return;
      checking = true;
      try {
        const next = await invoke<ApacheStatus>("apache_status");
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
  }, [pending]);

  async function act(action: "start_apache" | "stop_apache" | "open_demo") {
    setPending(action);
    setError(null);
    try {
      if (action === "open_demo") {
        await invoke(action);
      } else if (action === "start_apache") {
        setStatus(await invoke<ApacheStatus>(action, { projects }));
      } else {
        setStatus(await invoke<ApacheStatus>(action));
      }
    } catch (error) {
      setError(String(error));
    } finally {
      setPending(null);
    }
  }

  function saveProjects(next: Project[]) {
    localStorage.setItem(projectKey, JSON.stringify(next));
    setProjects(next);
    setProjectMessage("Sync .test domains to apply changes.");
  }

  async function projectAction(action: "validate_projects" | "open_project" | "sync_project_hosts", slug?: string) {
    setPending(action);
    setProjectError(null);
    setProjectMessage(null);
    try {
      if (action === "validate_projects") {
        const next = [...projects, {
          slug: draft.slug.trim().toLowerCase(), folder: draft.folder.trim(), webRoot: draft.webRoot.trim(),
        }];
        await invoke(action, { projects: next });
        saveProjects(next);
        setDraft(blankProject);
      } else if (action === "sync_project_hosts") {
        await invoke(action, { slugs: projects.map(project => project.slug) });
        setProjectMessage("Domains synced.");
      } else {
        await invoke(action, { slug });
      }
    } catch (error) {
      setProjectError(String(error));
    } finally {
      setPending(null);
    }
  }

  function removeProject(slug: string) {
    setProjectError(null);
    try {
      saveProjects(projects.filter(project => project.slug !== slug));
    } catch (error) {
      setProjectError(String(error));
    }
  }

  const running = status?.running ?? false;
  const canEditProjects = desktop && !pending && !running;
  const apacheChanging = pending === "start_apache" || pending === "stop_apache";
  const message = apacheChanging ? null : error ?? status?.error;
  const apacheLabel = pending === "start_apache" ? "Starting…"
    : pending === "stop_apache" ? "Stopping…"
    : status?.ready ? "Running"
    : running ? "PHP not ready"
    : message ? "Error"
    : !desktop ? "Unavailable"
    : !status ? "Checking…"
    : managed.apache && !managed.php ? "Needs PHP"
    : "Stopped";
  const apacheTone = apacheChanging ? "busy" : status?.ready ? "ready" : running || message ? "warning" : "neutral";

  return (
    <main className="dashboard">
      <header className="app-header">
        <h1 className="brand"><span className="brand-mark"><Icon name="bolt" /></span>Volt</h1>
        {!desktop && <span className="preview-label">Browser preview</span>}
      </header>

      <EnvironmentPanel
        onChange={setManaged}
        apacheRunning={running || apacheChanging}
        mysqlRunning={mysqlRunning}
        apacheControls={<>
          <ServiceSwitch
            label="Apache + PHP"
            status={apacheLabel}
            tone={apacheTone}
            running={running}
            disabled={!desktop || !status || !!pending || (!running && !apacheInstalled)}
            onChange={enabled => void act(enabled ? "start_apache" : "stop_apache")}
          />
          {message && <p className="notice error" role="alert">{message}</p>}
          {status?.logs && <details className="disclosure" open={!!status.error}>
            <summary>Apache logs</summary>
            <div className="disclosure-body"><pre>{status.logs}</pre></div>
          </details>}
        </>}
        mysqlControls={<MySqlPanel installed={!!managed.mysql} onRunningChange={setMysqlRunning} />}
        apacheAction={status?.ready && <button className="quiet-button" type="button" disabled={!!pending} onClick={() => void act("open_demo")}>
          Open demo <Icon name="external" />
        </button>}
      />
      <section aria-labelledby="projects-heading">
        <div className="section-heading">
          <div className="section-title">
            <h2 id="projects-heading">Projects</h2>
            <span className="section-count">{projects.length}<span className="sr-only"> {projects.length === 1 ? "project" : "projects"}</span></span>
          </div>
          <button type="button" title="Requires Windows administrator approval" disabled={!desktop || !!pending} onClick={() => void projectAction("sync_project_hosts")}>
            <Icon name="refresh" /> {pending === "sync_project_hosts" ? "Syncing…" : "Sync .test domains"}
          </button>
        </div>
        {projectMessage && <p className="notice" role="status">{projectMessage}</p>}
        {projectError && <p className="notice error" role="alert">{projectError}</p>}
        <div className="panel">
          {projects.length === 0 ? <p className="empty-state">No projects</p> : (
            <ul className="projects">
              {projects.map(project => (
                <li key={project.slug}>
                  <div className="project-info">
                    <strong className="project-address">{project.slug}.test:8080</strong>
                    <dl className="metadata">
                      <div><dt>Folder</dt><dd><code>{project.folder}</code></dd></div>
                      <div><dt>Web root</dt><dd><code>{project.webRoot}</code></dd></div>
                    </dl>
                  </div>
                  <div className="actions project-actions">
                    <button type="button" aria-label={`Open ${project.slug}`} disabled={!!pending || !status?.ready} onClick={() => void projectAction("open_project", project.slug)}>Open <Icon name="external" /></button>
                    <button className="quiet-button" type="button" aria-label={`Remove ${project.slug}`} disabled={!canEditProjects} onClick={() => removeProject(project.slug)}>Remove</button>
                  </div>
                </li>
              ))}
            </ul>
          )}
          <div className="project-footer">
            {running && <p className="note">Stop Apache to edit projects.</p>}
            <details className="disclosure add-project">
              <summary>Add project</summary>
              <form className="disclosure-body" onSubmit={event => {
                event.preventDefault();
                if (canEditProjects) void projectAction("validate_projects");
              }}>
                <fieldset disabled={!canEditProjects}>
                  <legend className="sr-only">Project details</legend>
                  <label className="project-folder-field">
                    Folder path
                    <input className="mono" value={draft.folder} onChange={event => setDraft({ ...draft, folder: event.target.value })} placeholder="D:\Projects\My Project" required spellCheck={false} />
                  </label>
                  <label>
                    Subdomain
                    <input className="mono" value={draft.slug} maxLength={50} onChange={event => setDraft({ ...draft, slug: event.target.value })} placeholder="my-project" required spellCheck={false} />
                  </label>
                  <label>
                    Web root
                    <input className="mono" value={draft.webRoot} onChange={event => setDraft({ ...draft, webRoot: event.target.value })} required spellCheck={false} aria-describedby="project-root-hint" />
                    <span className="note" id="project-root-hint">Use <code>.</code> only to serve the entire folder.</span>
                  </label>
                  <div className="actions"><button className="primary-button" type="submit">Add project</button></div>
                </fieldset>
              </form>
            </details>
          </div>
        </div>
      </section>
    </main>
  );
}

export default App;
