import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import "./App.css";

type View = "library" | "catalog" | "builder";

type AppSummary = {
  id: string;
  name: string;
  version: string;
  description: string;
  category: string;
  status: "stopped" | "starting" | "ready" | string;
  automaticUpdates: boolean;
  url: string;
  lastUpdateCheck: number | null;
};

type CatalogEntry = {
  id: string;
  name: string;
  version: string;
  description: string;
  category: string;
  homepage: string;
  license: string;
  memoryMb: number;
  diskMb: number;
  installed: boolean;
};

type SystemStatus = {
  engineAvailable: boolean;
  engineName: string;
  engineVersion: string | null;
  appDataDir: string;
  runtime: {
    installed: boolean;
    running: boolean;
    state: "not-installed" | "stopped" | "running" | string;
    version: string | null;
    details: string;
  };
};

type ActionResult = {
  id: string;
  status: string;
  message: string;
};

type BuilderAnalysis = {
  source: string;
  detectedName: string;
  services: Array<{ name: string; image: string | null; ports: number[]; volumes: string[]; environment: string[] }>;
  candidatePorts: number[];
  warnings: string[];
};

type SourceKind = "compose" | "image" | "github";

function slugify(value: string) {
  return value.toLowerCase().trim().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 70);
}

const glyphs: Record<string, React.ReactNode> = {
  library: <><rect x="4" y="5" width="16" height="14" rx="3"/><path d="M8 9h8M8 13h5"/></>,
  catalog: <><path d="M5 7.5 12 4l7 3.5v9L12 20l-7-3.5z"/><path d="m5 7.5 7 3.5 7-3.5M12 11v9"/></>,
  builder: <><path d="M12 3v4M12 17v4M3 12h4M17 12h4"/><circle cx="12" cy="12" r="5"/></>,
  play: <path d="m9 7 8 5-8 5z" fill="currentColor" stroke="none"/>,
  stop: <rect x="8" y="8" width="8" height="8" rx="1.5" fill="currentColor" stroke="none"/>,
  open: <><path d="M14 5h5v5M13 11l6-6"/><path d="M18 13v5a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/></>,
  dots: <><circle cx="6" cy="12" r="1" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1" fill="currentColor" stroke="none"/><circle cx="18" cy="12" r="1" fill="currentColor" stroke="none"/></>,
  refresh: <><path d="M19 8a7 7 0 1 0 .4 7"/><path d="M19 4v4h-4"/></>,
  logs: <><path d="M7 4h10a2 2 0 0 1 2 2v12H7a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z"/><path d="M8 9h8M8 13h6"/></>,
  trash: <><path d="M5 7h14M9 7V4h6v3M8 10v7M12 10v7M16 10v7M7 7l1 13h8l1-13"/></>,
  shield: <><path d="M12 3 5 6v5c0 4.4 2.9 8.4 7 10 4.1-1.6 7-5.6 7-10V6z"/><path d="m9 12 2 2 4-4"/></>,
  folder: <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>,
  spark: <><path d="m12 3 1.4 4.1L17 9l-3.6 1.9L12 15l-1.4-4.1L7 9l3.6-1.9z"/><path d="m18 15 .8 2.2L21 18l-2.2.8L18 21l-.8-2.2L15 18l2.2-.8z"/></>,
  close: <path d="m7 7 10 10M17 7 7 17"/>,
  chevron: <path d="m9 6 6 6-6 6"/>,
};

function Icon({ name, size = 20 }: { name: string; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {glyphs[name]}
    </svg>
  );
}

function AppMark({ small = false }: { small?: boolean }) {
  return <div className={`app-mark ${small ? "small" : ""}`}><span>O</span><i /></div>;
}

function statusLabel(status: string) {
  if (status === "ready") return "Running";
  if (status === "starting") return "Starting";
  return "Stopped";
}

function formatUpdate(timestamp: number | null) {
  if (!timestamp) return "Not checked yet";
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" })
    .format(new Date(timestamp * 1000));
}

function App() {
  const [view, setView] = useState<View>("library");
  const [apps, setApps] = useState<AppSummary[]>([]);
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [system, setSystem] = useState<SystemStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const [logs, setLogs] = useState<{ name: string; output: string } | null>(null);
  const [importPath, setImportPath] = useState("");
  const [sourceKind, setSourceKind] = useState<SourceKind>("compose");
  const [builderSource, setBuilderSource] = useState("");
  const [analysis, setAnalysis] = useState<BuilderAnalysis | null>(null);
  const [packageName, setPackageName] = useState("");
  const [packageId, setPackageId] = useState("");
  const [packageDescription, setPackageDescription] = useState("");
  const [packageHomepage, setPackageHomepage] = useState("");
  const [containerPort, setContainerPort] = useState("");
  const [secretKeys, setSecretKeys] = useState("");
  const [launcherAppId, setLauncherAppId] = useState<string | null | undefined>(undefined);
  const didCheckUpdates = useRef(false);
  const didCheckPackagerUpdate = useRef(false);
  const handledLinks = useRef(new Set<string>());

  const refresh = useCallback(async (quiet = false) => {
    try {
      const [nextApps, nextCatalog, nextSystem] = await Promise.all([
        invoke<AppSummary[]>("get_apps"),
        invoke<CatalogEntry[]>("get_catalog"),
        invoke<SystemStatus>("get_system_status"),
      ]);
      setApps(nextApps);
      setCatalog(nextCatalog);
      setSystem(nextSystem);
      if (!quiet) setError(null);
    } catch (reason) {
      if (!quiet) setError(String(reason));
    }
  }, []);

  useEffect(() => {
    refresh();
    const timer = window.setInterval(() => refresh(true), 5000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (launcherAppId !== null || didCheckUpdates.current || apps.length === 0) return;
    didCheckUpdates.current = true;
    invoke<ActionResult[]>("run_automatic_updates")
      .then((results) => {
        if (results.length) setNotice("Automatic updates completed.");
        refresh(true);
      })
      .catch(() => undefined);
  }, [apps.length, launcherAppId, refresh]);

  useEffect(() => {
    if (launcherAppId !== null || !import.meta.env.PROD || didCheckPackagerUpdate.current) return;
    didCheckPackagerUpdate.current = true;
    check()
      .then(async (update) => {
        if (!update) return;
        setNotice(`Updating Packager to ${update.version}…`);
        await update.downloadAndInstall();
        await relaunch();
      })
      .catch(() => undefined);
  }, [launcherAppId]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;

    async function handleLinks(urls: string[]) {
      for (const rawUrl of urls) {
        if (handledLinks.current.has(rawUrl)) continue;
        handledLinks.current.add(rawUrl);
        let parsed: URL;
        try {
          parsed = new URL(rawUrl);
        } catch {
          continue;
        }
        if (parsed.protocol !== "packager:" || parsed.hostname !== "open") continue;
        const id = parsed.pathname.replace(/^\//, "");
        if (!id) continue;

        await getCurrentWindow().hide();
        setView("library");
        setBusy(id);
        setNotice(`Starting ${id.replace(/-/g, " ")}…`);
        try {
          let installed = await invoke<AppSummary[]>("get_apps");
          let app = installed.find((item) => item.id === id);
          if (!app) throw new Error(`${id} is not installed in Packager`);
          if (app.status === "stopped") await invoke("start_app", { id });

          for (let attempt = 0; attempt < 90; attempt += 1) {
            if (disposed) return;
            installed = await invoke<AppSummary[]>("get_apps");
            app = installed.find((item) => item.id === id);
            if (app?.status === "ready") break;
            await new Promise((resolve) => window.setTimeout(resolve, 1000));
          }
          if (!app || app.status !== "ready") throw new Error(`${app?.name ?? id} did not become ready in time`);
          await invoke("open_app_window", { id });
          setNotice(`${app.name} is running.`);
          await refresh(true);
          await getCurrentWindow().hide();
        } catch (reason) {
          setError(String(reason));
          await getCurrentWindow().show();
          await getCurrentWindow().setFocus();
        } finally {
          setBusy(null);
        }
      }
    }

    invoke<string | null>("get_launcher_app_id")
      .then((id) => {
        setLauncherAppId(id);
        if (id) return handleLinks([`packager://open/${id}`]);
      })
      .catch(() => setLauncherAppId(null));
    getCurrent().then((urls) => urls && handleLinks(urls)).catch(() => undefined);
    onOpenUrl(handleLinks).then((stop) => { unlisten = stop; }).catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refresh]);

  async function act(command: string, id: string, args: Record<string, unknown> = {}) {
    setBusy(id);
    setMenuFor(null);
    setError(null);
    try {
      const result = await invoke<ActionResult>(command, { id, ...args });
      setNotice(result.message);
      await refresh(true);
      return result;
    } catch (reason) {
      setError(String(reason));
      throw reason;
    } finally {
      setBusy(null);
    }
  }

  async function manageRuntime() {
    const command = system?.runtime.installed ? "start_managed_runtime" : "install_managed_runtime";
    setBusy("runtime");
    setError(null);
    setNotice(system?.runtime.installed ? "Starting Packager Runtime…" : "Downloading the private runtime…");
    try {
      await invoke(command);
      setNotice("Packager Runtime is ready. Docker Desktop is not required.");
      await refresh(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  }

  async function stopRuntime() {
    setBusy("runtime");
    setError(null);
    try {
      await invoke("stop_managed_runtime");
      setNotice("Packager Runtime stopped. App data remains safe.");
      await refresh(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  }

  async function showLogs(app: AppSummary) {
    setBusy(app.id);
    setMenuFor(null);
    try {
      const output = await invoke<string>("get_app_logs", { id: app.id, lines: 300 });
      setLogs({ name: app.name, output: output || "No logs yet." });
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  }

  async function importRecipe() {
    if (!importPath.trim()) return;
    setBusy("import");
    setError(null);
    try {
      const result = await invoke<ActionResult>("import_package", { sourceDir: importPath.trim() });
      setNotice(result.message);
      setImportPath("");
      await refresh(true);
      setView("library");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  }

  async function analyzeSource() {
    if (!builderSource.trim()) return;
    setBusy("analyze");
    setError(null);
    setAnalysis(null);
    try {
      const result = await invoke<BuilderAnalysis>("analyze_package_source", { sourceKind, source: builderSource.trim() });
      setAnalysis(result);
      const name = result.detectedName.replace(/\b\w/g, (letter) => letter.toUpperCase());
      setPackageName(name);
      setPackageId(slugify(name));
      setContainerPort(String(result.candidatePorts[0] ?? ""));
      if (sourceKind === "github") setPackageHomepage(builderSource.trim());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  }

  async function createPackage() {
    const port = Number(containerPort);
    if (!analysis || !packageName.trim() || !packageId.trim() || !Number.isInteger(port) || port < 1 || port > 65535) return;
    setBusy("build");
    setError(null);
    try {
      const result = await invoke<ActionResult>("build_package", {
        request: {
          sourceKind,
          source: builderSource.trim(),
          id: packageId.trim(),
          name: packageName.trim(),
          description: packageDescription.trim(),
          homepage: packageHomepage.trim(),
          containerPort: port,
          secretKeys: secretKeys.split(",").map((key) => key.trim()).filter(Boolean),
        },
      });
      setNotice(result.message);
      await refresh(true);
      setView("library");
      setAnalysis(null);
      setBuilderSource("");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="shell" onClick={() => menuFor && setMenuFor(null)}>
      <aside className="sidebar">
        <div className="brand"><div className="brand-symbol"><span /><span /><span /></div><strong>Packager</strong><em>alpha</em></div>
        <nav>
          <button className={view === "library" ? "active" : ""} onClick={() => setView("library")}><Icon name="library" />Library{apps.length > 0 && <b>{apps.length}</b>}</button>
          <button className={view === "catalog" ? "active" : ""} onClick={() => setView("catalog")}><Icon name="catalog" />Catalog</button>
          <button className={view === "builder" ? "active" : ""} onClick={() => setView("builder")}><Icon name="builder" />Build a package</button>
        </nav>
        <div className="sidebar-bottom">
          <div className={`engine ${system?.engineAvailable ? "online" : "offline"}`}>
            <i /><div><strong>{system?.engineAvailable ? "Runtime ready" : system?.runtime.installed ? "Runtime sleeping" : "Runtime not installed"}</strong><span>{system?.engineAvailable ? system.runtime.version ?? system.engineName : "Private to Packager"}</span></div>{system?.runtime.running && <button disabled={busy === "runtime"} onClick={stopRuntime}>Stop</button>}
          </div>
          <div className="open-source"><Icon name="spark" size={17} /><span><strong>Open source</strong><small>Built for local software</small></span></div>
        </div>
      </aside>

      <main className="content">
        <header>
          <div>
            <p>{view === "library" ? "YOUR APPS" : view === "catalog" ? "COMMUNITY CATALOG" : "PACKAGE BUILDER"}</p>
            <h1>{view === "library" ? "Library" : view === "catalog" ? "Discover local apps" : "Turn a stack into an app"}</h1>
          </div>
          <button className="icon-button" title="Refresh" onClick={() => refresh()}><Icon name="refresh" /></button>
        </header>

        {error && <div className="banner error"><strong>Something needs attention</strong><span>{error}</span><button onClick={() => setError(null)}><Icon name="close" size={16} /></button></div>}
        {notice && <div className="banner notice"><Icon name="shield" size={18} /><span>{notice}</span><button onClick={() => setNotice(null)}><Icon name="close" size={16} /></button></div>}

        {system && !system.runtime.running && (
          <div className="runtime-setup"><div className="runtime-icon"><Icon name="spark" /></div><div><strong>{system.runtime.installed ? "Packager Runtime is ready to wake" : "One private runtime powers every packaged app"}</strong><span>{system.runtime.details}. It does not use Docker Desktop or alter your Docker contexts.</span></div><button className="primary" disabled={busy === "runtime"} onClick={manageRuntime}>{busy === "runtime" ? <><span className="spinner" />Working…</> : system.runtime.installed ? "Start runtime" : "Install runtime"}</button></div>
        )}

        {view === "library" && (
          <section className="library-view">
            {apps.length === 0 ? (
              <div className="empty-state"><div className="empty-orbit"><div className="brand-symbol"><span /><span /><span /></div></div><h2>Your local apps live here</h2><p>Install a package and Packager will keep its runtime, data, updates, and window together.</p><button className="primary" onClick={() => setView("catalog")}>Browse the catalog <Icon name="chevron" size={17} /></button></div>
            ) : (
              <div className="app-grid">
                {apps.map((app) => (
                  <article className="app-card" key={app.id}>
                    <div className="card-top"><AppMark /><div className="app-title"><h2>{app.name}</h2><span>{app.category} · v{app.version}</span></div><div className={`status ${app.status}`}><i />{statusLabel(app.status)}</div></div>
                    <p>{app.description}</p>
                    <div className="card-meta"><span><Icon name="shield" size={16} />Updates {app.automaticUpdates ? "on" : "off"}</span><span>{formatUpdate(app.lastUpdateCheck)}</span></div>
                    <div className="card-actions">
                      {app.status === "stopped" ? (
                        <button className="primary" disabled={busy === app.id || busy === "runtime"} onClick={() => act("start_app", app.id)}>{busy === app.id ? <span className="spinner" /> : <Icon name="play" size={17} />}Start</button>
                      ) : (
                        <button className="primary" disabled={busy === app.id || app.status !== "ready"} onClick={() => act("open_app_window", app.id)}>{busy === app.id || app.status === "starting" ? <span className="spinner" /> : <Icon name="open" size={17} />}{app.status === "starting" ? "Starting…" : "Open"}</button>
                      )}
                      {app.status !== "stopped" && <button className="secondary" disabled={busy === app.id} onClick={() => act("stop_app", app.id)}><Icon name="stop" size={16} />Stop</button>}
                      <div className="menu-wrap"><button className="icon-button" onClick={(event) => { event.stopPropagation(); setMenuFor(menuFor === app.id ? null : app.id); }}><Icon name="dots" /></button>{menuFor === app.id && <div className="menu" onClick={(event) => event.stopPropagation()}><button onClick={() => act("update_app", app.id)}><Icon name="refresh" size={16} />Check for updates</button><button onClick={() => showLogs(app)}><Icon name="logs" size={16} />View logs</button><button onClick={() => act("set_automatic_updates", app.id, { enabled: !app.automaticUpdates })}><Icon name="shield" size={16} />Turn updates {app.automaticUpdates ? "off" : "on"}</button><hr /><button className="danger" onClick={() => act("uninstall_app", app.id, { deleteData: false })}><Icon name="trash" size={16} />Uninstall, keep data</button></div>}</div>
                    </div>
                  </article>
                ))}
                <button className="add-card" onClick={() => setView("catalog")}><span>+</span><strong>Add another app</strong><small>Browse community packages</small></button>
              </div>
            )}
          </section>
        )}

        {view === "catalog" && (
          <section className="catalog-view">
            <div className="featured-copy"><span>FIRST-PARTY RECIPE</span><h2>Useful software, without the installation ritual.</h2><p>Catalog recipes are readable, versioned, and isolated. Packager manages the machinery while you use the app.</p></div>
            <div className="catalog-grid">
              {catalog.map((entry) => (
                <article className="catalog-card" key={entry.id}>
                  <div className="catalog-art"><AppMark /><span>OPEN<br />NOTEBOOK</span><div className="document-lines"><i /><i /><i /></div></div>
                  <div className="catalog-body"><div className="eyebrow"><span>{entry.category}</span><small>{entry.license}</small></div><h2>{entry.name}</h2><p>{entry.description}</p><div className="requirements"><span>{Math.round(entry.memoryMb / 1024)} GB memory</span><i /> <span>{(entry.diskMb / 1024).toFixed(0)} GB disk</span><i /><span>v{entry.version}</span></div><button className={entry.installed ? "secondary full" : "primary full"} disabled={entry.installed || busy === entry.id} onClick={() => act("install_app", entry.id)}>{busy === entry.id ? <span className="spinner" /> : entry.installed ? "Installed" : "Install package"}</button></div>
                </article>
              ))}
            </div>
          </section>
        )}

        {view === "builder" && (
          <section className="builder-view">
            <div className="builder-intro"><span className="step-number">01</span><div><h2>Choose what already runs</h2><p>Start with a Compose project, one container image, or a public GitHub repository. Packager reads the source and proposes a safe desktop package.</p></div></div>
            <div className="source-tabs">
              {(["compose", "image", "github"] as SourceKind[]).map((kind) => <button key={kind} className={sourceKind === kind ? "active" : ""} onClick={() => { setSourceKind(kind); setAnalysis(null); setBuilderSource(""); }}>{kind === "compose" ? "Compose folder" : kind === "image" ? "Docker image" : "GitHub repository"}</button>)}
            </div>
            <div className="import-panel source-panel"><label>{sourceKind === "compose" ? "Folder or Compose file" : sourceKind === "image" ? "Container image" : "Public repository URL"}</label><div className="path-input"><Icon name={sourceKind === "compose" ? "folder" : sourceKind === "github" ? "open" : "catalog"} size={19} /><input value={builderSource} onChange={(event) => setBuilderSource(event.target.value)} placeholder={sourceKind === "compose" ? "/Users/you/Projects/my-stack" : sourceKind === "image" ? "ghcr.io/owner/app:latest" : "https://github.com/owner/repository"} onKeyDown={(event) => event.key === "Enter" && analyzeSource()} /><button className="primary" disabled={!builderSource.trim() || busy === "analyze"} onClick={analyzeSource}>{busy === "analyze" ? <><span className="spinner" />Inspecting…</> : "Analyze"}</button></div><small>Analysis does not start containers. Public GitHub sources are downloaded to a temporary Packager cache.</small></div>

            {analysis && <>
              <div className="builder-step"><span className="step-number">02</span><div><h2>Review what Packager found</h2><p>{analysis.services.length} service{analysis.services.length === 1 ? "" : "s"} · {analysis.candidatePorts.length ? `web port candidates ${analysis.candidatePorts.join(", ")}` : "no published ports detected"}</p></div></div>
              <div className="analysis-grid">{analysis.services.map((service) => <div className="service-row" key={service.name}><div><strong>{service.name}</strong><span>{service.image ?? "Built from local source"}</span></div><small>{service.ports.length ? `Ports ${service.ports.join(", ")}` : "No exposed port"}</small></div>)}</div>
              {analysis.warnings.length > 0 && <div className="review-warnings">{analysis.warnings.map((warning) => <p key={warning}><Icon name="shield" size={15} />{warning}</p>)}</div>}

              <div className="builder-step"><span className="step-number">03</span><div><h2>Name the app and confirm its web port</h2><p>Packager rewrites every published port to loopback with collision-free allocation. Comma-separated secret variables are generated into Keychain.</p></div></div>
              <div className="package-form">
                <label><span>App name</span><input value={packageName} onChange={(event) => { setPackageName(event.target.value); setPackageId(slugify(event.target.value)); }} /></label>
                <label><span>Package id</span><input value={packageId} onChange={(event) => setPackageId(slugify(event.target.value))} /></label>
                <label><span>Web container port</span><input inputMode="numeric" value={containerPort} onChange={(event) => setContainerPort(event.target.value.replace(/\D/g, ""))} placeholder="3000" /></label>
                <label><span>Homepage (optional)</span><input value={packageHomepage} onChange={(event) => setPackageHomepage(event.target.value)} placeholder="https://…" /></label>
                <label className="wide"><span>Description (optional)</span><input value={packageDescription} onChange={(event) => setPackageDescription(event.target.value)} placeholder="What this app does" /></label>
                <label className="wide"><span>Secret environment keys (optional)</span><input value={secretKeys} onChange={(event) => setSecretKeys(event.target.value.toUpperCase())} placeholder="API_KEY, ENCRYPTION_SECRET" /></label>
                <button className="primary build-button" disabled={busy === "build" || !packageName || !packageId || !containerPort} onClick={createPackage}>{busy === "build" ? <><span className="spinner" />Building package…</> : <>Generate & install <Icon name="chevron" size={16} /></>}</button>
              </div>
            </>}

            <details className="advanced-import"><summary>Already have a packager.yml?</summary><div className="import-panel"><label>Package folder</label><div className="path-input"><Icon name="folder" size={19} /><input value={importPath} onChange={(event) => setImportPath(event.target.value)} placeholder="/Users/you/Projects/my-package" onKeyDown={(event) => event.key === "Enter" && importRecipe()} /><button className="secondary" disabled={!importPath.trim() || busy === "import"} onClick={importRecipe}>{busy === "import" ? <span className="spinner" /> : "Validate & import"}</button></div></div></details>
            <div className="builder-notes"><div><Icon name="shield" /><h3>Safe by default</h3><p>Localhost-only ports, constrained paths, and explicit package metadata.</p></div><div><Icon name="refresh" /><h3>Updates included</h3><p>Pull fresh images on schedule and recreate only when needed.</p></div><div><Icon name="open" /><h3>A real app window</h3><p>The local interface opens separately from Packager’s control panel.</p></div></div>
          </section>
        )}
      </main>

      {logs && <div className="modal-backdrop" onClick={() => setLogs(null)}><div className="logs-modal" onClick={(event) => event.stopPropagation()}><div className="modal-head"><div><Icon name="logs" /><span><strong>{logs.name}</strong><small>Recent runtime logs</small></span></div><button className="icon-button" onClick={() => setLogs(null)}><Icon name="close" /></button></div><pre>{logs.output}</pre></div></div>}
    </div>
  );
}

export default App;
