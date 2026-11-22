import { invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import EnvironmentPanel, { type ManagedFolders } from "./EnvironmentPanel";
import MySqlPanel from "./MySqlPanel";
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
    setProjectMessage("Project list updated. Sync .test Domains to update Windows.");
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
        setProjectMessage("Windows hosts file synced.");
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
  const apacheLabel = pending === "start_apache" ? "Starting Apache with PHP…"
    : pending === "stop_apache" ? "Stopping Apache…"
    : status?.ready ? "Apache + PHP running at 127.0.0.1:8080"
    : running ? "Apache is running, but PHP isn’t ready"
    : !apacheInstalled ? "Select installed Apache and PHP versions to enable Apache."
    : "Apache stopped";
  const message = apacheChanging ? null : error ?? status?.error;

  return (
    <main className="dashboard">
      <header>
        <h1>Volt</h1>
        <p>Local development for Windows</p>
      </header>

      <EnvironmentPanel
        onChange={setManaged}
        apacheRunning={running || apacheChanging}
        mysqlRunning={mysqlRunning}
        apacheControls={<>
          <div className="service-actions">
            <label className="service-switch">
              <input type="checkbox" role="switch" checked={running} disabled={!desktop || !status || !!pending || (!running && !apacheInstalled)} onChange={event => void act(event.target.checked ? "start_apache" : "stop_apache")} />
              Enable Apache (with PHP)
            </label>
            <button type="button" disabled={!!pending || !status?.ready} onClick={() => void act("open_demo")}>Open Demo</button>
          </div>
          <p className="status" role="status">{apacheLabel}</p>
          {message && <p className="error" role="alert">{message}</p>}
          {status?.logs && <details open={!!status.error}>
            <summary>Apache logs</summary>
            <pre>{status.logs}</pre>
          </details>}
        </>}
        mysqlControls={<MySqlPanel installed={!!managed.mysql} onRunningChange={setMysqlRunning} />}
      />
      <section aria-labelledby="projects-heading">
        <h2 id="projects-heading">Projects</h2>
        <p className="note">All projects share the running PHP version. Stop Apache to add or remove projects.</p>
        {projects.length === 0 ? <p className="note">No projects registered.</p> : (
          <ul className="projects">
            {projects.map(project => (
              <li key={project.slug}>
                <div>
                  <strong>{project.slug}.test:8080</strong>
                  <p>{project.folder} · Web root: {project.webRoot}</p>
                </div>
                <div className="actions">
                  <button type="button" aria-label={`Open ${project.slug}`} disabled={!!pending || !status?.ready} onClick={() => void projectAction("open_project", project.slug)}>Open</button>
                  <button type="button" aria-label={`Remove ${project.slug}`} disabled={!canEditProjects} onClick={() => removeProject(project.slug)}>Remove</button>
                </div>
              </li>
            ))}
          </ul>
        )}
        <form onSubmit={event => {
          event.preventDefault();
          if (canEditProjects) void projectAction("validate_projects");
        }}>
          <fieldset disabled={!canEditProjects}>
            <legend>Add a project</legend>
            <label>
              Project name (used in the URL)
              <input value={draft.slug} maxLength={50} onChange={event => setDraft({ ...draft, slug: event.target.value })} placeholder="my-project" required spellCheck={false} />
            </label>
            <label>
              Absolute project folder
              <input value={draft.folder} onChange={event => setDraft({ ...draft, folder: event.target.value })} placeholder="D:\Projects\My Project" required spellCheck={false} />
            </label>
            <label>
              Web root inside the project (use public or . for the whole folder)
              <input value={draft.webRoot} onChange={event => setDraft({ ...draft, webRoot: event.target.value })} required spellCheck={false} />
            </label>
            <div className="actions"><button type="submit">Add Project</button></div>
          </fieldset>
        </form>
        <p className="note">Sync once after adding or removing a project. Windows will ask for administrator approval.</p>
        <button className="sync-button" type="button" disabled={!desktop || !!pending} onClick={() => void projectAction("sync_project_hosts")}>
          {pending === "sync_project_hosts" ? "Syncing…" : "Sync .test Domains"}
        </button>
        {projectMessage && <p className="note" role="status">{projectMessage}</p>}
        {projectError && <p className="error" role="alert">{projectError}</p>}
      </section>
    </main>
  );
}

export default App;
