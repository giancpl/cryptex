import { useCallback, useEffect, useRef, useState } from "react";
import type { EditorState } from "@codemirror/state";
import type { FileTreeEntry } from "./bindings/FileTreeEntry";
import type { FileTreePage } from "./bindings/FileTreePage";
import type { ProjectSummary } from "./bindings/ProjectSummary";
import {
  backendClient,
  pickProjectDirectory,
  type BackendClient,
} from "./api/client";
import { CodeEditor } from "./editor/CodeEditor";
import { createLatexEditorState } from "./editor/editorState";

interface OpenDocument {
  path: string;
  fingerprint: string;
  state: EditorState;
  revision: number;
  savedRevision: number;
  saveStatus: "clean" | "dirty" | "saving" | "error";
  saveError: string | undefined;
}

interface AppProps {
  client?: BackendClient;
  pickDirectory?: () => Promise<string | null>;
}

export function App({
  client = backendClient,
  pickDirectory = pickProjectDirectory,
}: AppProps) {
  const [project, setProject] = useState<ProjectSummary | null>(null);
  const [directories, setDirectories] = useState<Record<string, FileTreePage>>(
    {},
  );
  const [expanded, setExpanded] = useState<Set<string>>(new Set([""]));
  const [showHidden, setShowHidden] = useState(false);
  const [showGenerated, setShowGenerated] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [documents, setDocuments] = useState<Record<string, OpenDocument>>({});
  const [activePath, setActivePath] = useState<string | null>(null);
  const documentsRef = useRef(documents);
  const directoriesRef = useRef(directories);
  const saveQueues = useRef(new Map<string, Promise<void>>());
  const autosaveTimers = useRef(
    new Map<string, { revision: number; timer: number }>(),
  );

  useEffect(() => {
    documentsRef.current = documents;
  }, [documents]);

  useEffect(() => {
    directoriesRef.current = directories;
  }, [directories]);

  useEffect(() => {
    if (!project) return;
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void client
      .onProjectFileChange((change) => {
        if (
          disposed ||
          change.projectId !== project.projectId ||
          change.selfWrite
        )
          return;
        const loadedDirectories = Object.keys(directoriesRef.current);
        void Promise.all(
          loadedDirectories.map((path) =>
            client.listDirectory(project.projectId, path),
          ),
        )
          .then((pages) => {
            if (disposed) return;
            setDirectories(
              Object.fromEntries(
                loadedDirectories.map((path, index) => [path, pages[index]]),
              ),
            );
          })
          .catch((reason: unknown) => {
            if (!disposed) setError(errorMessage(reason));
          });
      })
      .then((stop) => {
        if (disposed) stop();
        else unsubscribe = stop;
      })
      .catch((reason: unknown) => {
        if (!disposed) setError(errorMessage(reason));
      });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [client, project]);

  const saveDocument = useCallback(
    (path: string): Promise<void> => {
      if (!project) return Promise.resolve();
      const previous = saveQueues.current.get(path) ?? Promise.resolve();
      const queued = previous
        .catch(() => undefined)
        .then(async () => {
          const snapshot = documentsRef.current[path];
          if (!snapshot || snapshot.revision <= snapshot.savedRevision) return;
          const revision = snapshot.revision;
          const expectedFingerprint = snapshot.fingerprint;
          const text = snapshot.state.doc.toString();
          setDocuments((current) =>
            current[path]
              ? {
                  ...current,
                  [path]: {
                    ...current[path],
                    saveStatus: "saving",
                    saveError: undefined,
                  },
                }
              : current,
          );
          try {
            const result = await client.writeTextFile(
              project.projectId,
              path,
              text,
              expectedFingerprint,
            );
            setDocuments((current) => {
              const latest = current[path];
              if (!latest || latest.fingerprint !== expectedFingerprint)
                return current;
              const savedRevision = Math.max(latest.savedRevision, revision);
              return {
                ...current,
                [path]: {
                  ...latest,
                  fingerprint: result.fingerprint,
                  savedRevision,
                  saveStatus:
                    latest.revision > savedRevision ? "dirty" : "clean",
                  saveError: undefined,
                },
              };
            });
          } catch (reason) {
            setDocuments((current) =>
              current[path]
                ? {
                    ...current,
                    [path]: {
                      ...current[path],
                      saveStatus: "error",
                      saveError: errorMessage(reason),
                    },
                  }
                : current,
            );
          }
        });
      saveQueues.current.set(path, queued);
      void queued.finally(() => {
        if (saveQueues.current.get(path) === queued)
          saveQueues.current.delete(path);
      });
      return queued;
    },
    [client, project],
  );

  useEffect(() => {
    for (const [path, document] of Object.entries(documents)) {
      const scheduled = autosaveTimers.current.get(path);
      if (document.saveStatus !== "dirty") {
        if (scheduled) window.clearTimeout(scheduled.timer);
        autosaveTimers.current.delete(path);
      } else if (!scheduled || scheduled.revision !== document.revision) {
        if (scheduled) window.clearTimeout(scheduled.timer);
        const timer = window.setTimeout(() => {
          autosaveTimers.current.delete(path);
          void saveDocument(path);
        }, 750);
        autosaveTimers.current.set(path, {
          revision: document.revision,
          timer,
        });
      }
    }
  }, [documents, saveDocument]);

  useEffect(() => {
    const timers = autosaveTimers.current;
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (
        Object.values(documentsRef.current).some(
          (document) => document.revision > document.savedRevision,
        )
      )
        event.preventDefault();
    };
    window.addEventListener("beforeunload", beforeUnload);
    return () => {
      window.removeEventListener("beforeunload", beforeUnload);
      for (const scheduled of timers.values())
        window.clearTimeout(scheduled.timer);
    };
  }, []);

  async function openProject() {
    if (
      Object.values(documents).some(
        (document) => document.revision > document.savedRevision,
      ) &&
      !window.confirm("Discard unsaved changes and open another project?")
    )
      return;
    const selected = await pickDirectory();
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const opened = await client.openProject(selected);
      const root = await client.listDirectory(opened.projectId, "");
      setProject(opened);
      setDirectories({ "": root });
      setExpanded(new Set([""]));
      setDocuments({});
      setActivePath(null);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function toggleDirectory(path: string) {
    if (!project) return;
    if (expanded.has(path)) {
      setExpanded((current) => without(current, path));
      return;
    }
    try {
      if (!directories[path]) {
        const page = await client.listDirectory(project.projectId, path);
        setDirectories((current) => ({ ...current, [path]: page }));
      }
      setExpanded((current) => new Set(current).add(path));
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  async function openDocument(path: string) {
    if (!project) return;
    if (documents[path]) {
      setActivePath(path);
      return;
    }
    try {
      const loaded = await client.readTextFile(project.projectId, path);
      const state = createLatexEditorState(
        loaded.text,
        (next, changed) => {
          setDocuments((current) => {
            const document = current[path];
            return document
              ? {
                  ...current,
                  [path]: {
                    ...document,
                    state: next,
                    revision: changed
                      ? document.revision + 1
                      : document.revision,
                    saveStatus: changed ? "dirty" : document.saveStatus,
                  },
                }
              : current;
          });
        },
        () => {
          void saveDocument(path);
        },
      );
      setDocuments((current) => ({
        ...current,
        [path]: {
          path,
          fingerprint: loaded.fingerprint,
          state,
          revision: 0,
          savedRevision: 0,
          saveStatus: "clean",
          saveError: undefined,
        },
      }));
      setActivePath(path);
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  function closeDocument(path: string) {
    const document = documents[path];
    if (
      document &&
      document.revision > document.savedRevision &&
      !window.confirm(`Discard unsaved changes to ${path}?`)
    )
      return;
    setDocuments((current) => {
      const next = { ...current };
      delete next[path];
      return next;
    });
    if (activePath === path) {
      const remaining = Object.keys(documents).filter(
        (candidate) => candidate !== path,
      );
      setActivePath(remaining.at(-1) ?? null);
    }
  }

  return (
    <main className="workspace" aria-label="CrypTex workspace">
      <header className="titlebar">
        <span className="wordmark">CrypTex</span>
        <span className="status">
          {project?.canonicalRoot ?? "No project open"}
        </span>
      </header>
      <div className="panes">
        <section className="pane project-pane" aria-labelledby="pane-project">
          <div className="pane-heading">
            <h1 id="pane-project">Project</h1>
            <button
              type="button"
              onClick={() => void openProject()}
              disabled={busy}
            >
              {busy ? "Opening…" : "Open folder"}
            </button>
          </div>
          {error ? <p role="alert">{error}</p> : null}
          {project ? (
            <>
              <h2>{project.name}</h2>
              <div className="tree-options">
                <label>
                  <input
                    type="checkbox"
                    checked={showHidden}
                    onChange={(event) => setShowHidden(event.target.checked)}
                  />{" "}
                  Hidden
                </label>
                <label>
                  <input
                    type="checkbox"
                    checked={showGenerated}
                    onChange={(event) => setShowGenerated(event.target.checked)}
                  />{" "}
                  Build files
                </label>
              </div>
              <FileTree
                directory=""
                directories={directories}
                expanded={expanded}
                showHidden={showHidden}
                showGenerated={showGenerated}
                onToggle={toggleDirectory}
                onOpen={openDocument}
              />
            </>
          ) : (
            <p>Open a LaTeX project to browse its files.</p>
          )}
        </section>
        <section className="pane editor-pane" aria-labelledby="pane-editor">
          <h1 id="pane-editor" className="visually-hidden">
            Editor
          </h1>
          <div
            className="editor-tabs"
            role="tablist"
            aria-label="Open documents"
          >
            {Object.values(documents).map((document) => (
              <div
                className={`editor-tab ${activePath === document.path ? "active" : ""}`}
                key={document.path}
              >
                <button
                  type="button"
                  role="tab"
                  aria-selected={activePath === document.path}
                  onClick={() => setActivePath(document.path)}
                >
                  {document.path.split("/").at(-1)}
                  {document.revision > document.savedRevision ? " •" : ""}
                </button>
                <button
                  type="button"
                  aria-label={`Close ${document.path}`}
                  onClick={() => closeDocument(document.path)}
                >
                  ×
                </button>
              </div>
            ))}
          </div>
          {activePath && documents[activePath] ? (
            <div className="save-bar">
              <button
                type="button"
                onClick={() => void saveDocument(activePath)}
                disabled={documents[activePath].saveStatus === "saving"}
              >
                Save
              </button>
              <span role="status">
                {documents[activePath].saveStatus === "error"
                  ? documents[activePath].saveError
                  : documents[activePath].saveStatus}
              </span>
            </div>
          ) : null}
          {activePath && documents[activePath] ? (
            <CodeEditor key={activePath} state={documents[activePath].state} />
          ) : (
            <p>Select a text file to begin editing.</p>
          )}
        </section>
        <section className="pane" aria-labelledby="pane-pdf">
          <h1 id="pane-pdf">PDF</h1>
          <p>A successful build will appear here.</p>
        </section>
      </div>
    </main>
  );
}

interface FileTreeProps {
  directory: string;
  directories: Record<string, FileTreePage>;
  expanded: Set<string>;
  showHidden: boolean;
  showGenerated: boolean;
  onToggle(path: string): Promise<void>;
  onOpen(path: string): Promise<void>;
}

function FileTree(props: FileTreeProps) {
  const page = props.directories[props.directory];
  if (!page) return null;
  const entries = page.entries.filter(
    (entry) =>
      (props.showHidden || !entry.hidden) &&
      (props.showGenerated || !entry.generated),
  );
  return (
    <ul className="file-tree">
      {entries.map((entry) => (
        <FileTreeItem key={entry.relativePath} entry={entry} {...props} />
      ))}
      {page.truncated ? (
        <li className="tree-note">Directory limited to 5,000 entries.</li>
      ) : null}
    </ul>
  );
}

function FileTreeItem({
  entry,
  ...props
}: FileTreeProps & { entry: FileTreeEntry }) {
  const isDirectory = entry.kind === "directory";
  const isExpanded = props.expanded.has(entry.relativePath);
  return (
    <li>
      <button
        className={`tree-entry ${entry.accessible ? "" : "inaccessible"}`}
        type="button"
        disabled={!entry.accessible}
        aria-expanded={isDirectory ? isExpanded : undefined}
        onClick={() =>
          void (isDirectory
            ? props.onToggle(entry.relativePath)
            : props.onOpen(entry.relativePath))
        }
      >
        <span aria-hidden="true">
          {isDirectory ? (isExpanded ? "▾" : "▸") : "·"}
        </span>
        {entry.name}
        {entry.isSymlink ? " ↗" : ""}
      </button>
      {isDirectory && isExpanded ? (
        <FileTree {...props} directory={entry.relativePath} />
      ) : null}
    </li>
  );
}

function without(values: Set<string>, value: string): Set<string> {
  const next = new Set(values);
  next.delete(value);
  return next;
}

function errorMessage(reason: unknown): string {
  if (
    typeof reason === "object" &&
    reason &&
    "message" in reason &&
    typeof reason.message === "string"
  )
    return reason.message;
  return reason instanceof Error
    ? reason.message
    : "Unable to open the project.";
}
