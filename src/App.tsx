import { useState } from "react";
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
  dirty: boolean;
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

  async function openProject() {
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
      const state = createLatexEditorState(loaded.text, (next, changed) => {
        setDocuments((current) => {
          const document = current[path];
          return document
            ? {
                ...current,
                [path]: {
                  ...document,
                  state: next,
                  dirty: document.dirty || changed,
                },
              }
            : current;
        });
      });
      setDocuments((current) => ({
        ...current,
        [path]: { path, fingerprint: loaded.fingerprint, state, dirty: false },
      }));
      setActivePath(path);
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  function closeDocument(path: string) {
    const document = documents[path];
    if (
      document?.dirty &&
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
                  {document.dirty ? " •" : ""}
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
