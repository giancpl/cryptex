import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { EditorState } from "@codemirror/state";
import type { BuildOutput } from "./bindings/BuildOutput";
import type { BuildState } from "./bindings/BuildState";
import type { CatalogSearchHit } from "./bindings/CatalogSearchHit";
import type { CommandContext } from "./bindings/CommandContext";
import type { Diagnostic } from "./bindings/Diagnostic";
import type { DiagnosticSeverity } from "./bindings/DiagnosticSeverity";
import type { FileTreeEntry } from "./bindings/FileTreeEntry";
import type { FileTreePage } from "./bindings/FileTreePage";
import type { RecoveryInventory } from "./bindings/RecoveryInventory";
import type { RecoverySnapshot } from "./bindings/RecoverySnapshot";
import type { RootDocumentCandidates } from "./bindings/RootDocumentCandidates";
import type { ProjectSummary } from "./bindings/ProjectSummary";
import type { ProjectIndex } from "./bindings/ProjectIndex";
import type { IndexRecord } from "./bindings/IndexRecord";
import type { TextDocument } from "./bindings/TextDocument";
import type { SynctexPosition } from "./bindings/SynctexPosition";
import {
  backendClient,
  pickProjectDirectory,
  type BackendClient,
} from "./api/client";
import { CodeEditor } from "./editor/CodeEditor";
import { PdfViewer } from "./pdf/PdfViewer";
import { createLatexEditorState } from "./editor/editorState";
import { CommandFinder } from "./finder/CommandFinder";

interface OpenDocument {
  path: string;
  fingerprint: string;
  state: EditorState;
  revision: number;
  savedRevision: number;
  saveStatus: "clean" | "dirty" | "saving" | "error" | "conflict";
  saveError: string | undefined;
  generation: number;
  conflict: DocumentConflict | undefined;
}

interface DocumentConflict {
  disk: TextDocument | undefined;
  message: string;
  comparing: boolean;
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
  const [projectIndex, setProjectIndex] = useState<ProjectIndex | null>(null);
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
  const [recoveryInventory, setRecoveryInventory] =
    useState<RecoveryInventory | null>(null);
  const [reviewingRecovery, setReviewingRecovery] =
    useState<RecoverySnapshot | null>(null);
  const [recoveryDisk, setRecoveryDisk] = useState<TextDocument | null>(null);
  const [rootDocuments, setRootDocuments] =
    useState<RootDocumentCandidates | null>(null);
  const [buildState, setBuildState] = useState<BuildState | null>(null);
  const [buildLog, setBuildLog] = useState("");
  const [rawBuildLog, setRawBuildLog] = useState<string | null>(null);
  const [pdfPreview, setPdfPreview] = useState<{
    projectId: string;
    operationId: string;
    data: Uint8Array;
  } | null>(null);
  const [pdfError, setPdfError] = useState<string | null>(null);
  const [synctexTarget, setSynctexTarget] = useState<
    (SynctexPosition & { requestId: number }) | null
  >(null);
  const [synctexMessage, setSynctexMessage] = useState<string | null>(null);
  const [diagnosticFilter, setDiagnosticFilter] = useState<
    DiagnosticSeverity | "all"
  >("all");
  const [diagnosticEpoch, setDiagnosticEpoch] = useState<number | null>(null);
  const [finderOpen, setFinderOpen] = useState(false);
  const [editorInsertion, setEditorInsertion] = useState<{
    path: string;
    request: number;
    snippet: string;
  } | null>(null);
  const [editorNavigation, setEditorNavigation] = useState<{
    path: string;
    line: number;
    column: number | null;
    request: number;
  } | null>(null);
  const documentsRef = useRef(documents);
  const directoriesRef = useRef(directories);
  const buildOperationRef = useRef<string | null>(null);
  const pdfRequestRef = useRef(0);
  const synctexRequestRef = useRef(0);
  const insertionRequestRef = useRef(0);
  const projectEpochRef = useRef(0);
  const buildEpochsRef = useRef(new Map<string, number>());
  const saveQueues = useRef(new Map<string, Promise<void>>());
  const recoveryTimers = useRef(
    new Map<string, { revision: number; timer: number }>(),
  );
  const autosaveTimers = useRef(
    new Map<string, { revision: number; timer: number }>(),
  );

  useEffect(() => {
    documentsRef.current = documents;
  }, [documents]);

  useEffect(() => {
    const openFinder = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        if (project && activePath) setFinderOpen(true);
      }
    };
    window.addEventListener("keydown", openFinder);
    return () => window.removeEventListener("keydown", openFinder);
  }, [activePath, project]);

  useEffect(() => {
    directoriesRef.current = directories;
  }, [directories]);

  const diagnosticsStale = Boolean(
    buildState &&
    isTerminalBuild(buildState) &&
    diagnosticEpoch !== null &&
    projectEpochRef.current !== diagnosticEpoch,
  );
  const visibleDiagnostics = useMemo(
    () =>
      (buildState?.diagnostics ?? []).filter(
        (diagnostic) =>
          diagnosticFilter === "all" ||
          diagnostic.severity === diagnosticFilter,
      ),
    [buildState, diagnosticFilter],
  );
  const activeDiagnosticMarkers = useMemo(
    () =>
      diagnosticsStale || !activePath
        ? []
        : (buildState?.diagnostics ?? [])
            .filter(
              (diagnostic) => diagnostic.source?.relativePath === activePath,
            )
            .map((diagnostic) => ({
              line: diagnostic.source!.startLine,
              severity: diagnostic.severity,
            })),
    [activePath, buildState, diagnosticsStale],
  );

  const saveDocument = useCallback(
    (path: string): Promise<void> => {
      if (!project) return Promise.resolve();
      const previous = saveQueues.current.get(path) ?? Promise.resolve();
      const queued = previous
        .catch(() => undefined)
        .then(async () => {
          const snapshot = documentsRef.current[path];
          if (
            !snapshot ||
            snapshot.saveStatus === "conflict" ||
            snapshot.revision <= snapshot.savedRevision
          )
            return;
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
            if (documentsRef.current[path]?.revision === revision) {
              try {
                await client.deleteRecoverySnapshot(project.projectId, path);
                setRecoveryInventory((current) =>
                  current
                    ? {
                        ...current,
                        snapshots: current.snapshots.filter(
                          (snapshot) => snapshot.relativePath !== path,
                        ),
                      }
                    : current,
                );
              } catch (reason) {
                setError(errorMessage(reason));
              }
            }
            setDocuments((current) => {
              const latest = current[path];
              if (
                !latest ||
                latest.saveStatus === "conflict" ||
                latest.fingerprint !== expectedFingerprint
              )
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
            let disk: TextDocument | undefined;
            const stale = errorCode(reason) === "STALE_FINGERPRINT";
            if (stale) {
              try {
                disk = await client.readTextFile(project.projectId, path);
              } catch {
                // A deleted or inaccessible disk version is still a conflict.
              }
            }
            setDocuments((current) =>
              current[path]
                ? {
                    ...current,
                    [path]: {
                      ...current[path],
                      saveStatus: stale ? "conflict" : "error",
                      saveError: stale ? undefined : errorMessage(reason),
                      conflict: stale
                        ? {
                            disk,
                            message:
                              "The file changed on disk while it was being saved.",
                            comparing: false,
                          }
                        : current[path].conflict,
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

  const makeEditorState = useCallback(
    (path: string, text: string) =>
      createLatexEditorState(
        text,
        (next, changed) => {
          if (changed) projectEpochRef.current += 1;
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
                    saveStatus: changed
                      ? document.conflict
                        ? "conflict"
                        : "dirty"
                      : document.saveStatus,
                  },
                }
              : current;
          });
        },
        () => {
          void saveDocument(path);
        },
      ),
    [saveDocument],
  );

  const reconcileExternalDocument = useCallback(
    async (path: string) => {
      if (!project || !documentsRef.current[path]) return;
      let disk: TextDocument | undefined;
      try {
        disk = await client.readTextFile(project.projectId, path);
      } catch {
        // Removal and permission changes are represented by an unavailable disk copy.
      }
      setDocuments((current) => {
        const document = current[path];
        if (!document || disk?.fingerprint === document.fingerprint)
          return current;
        const dirty =
          document.revision > document.savedRevision ||
          document.saveStatus === "saving" ||
          document.saveStatus === "error" ||
          document.saveStatus === "conflict";
        if (dirty || !disk) {
          return {
            ...current,
            [path]: {
              ...document,
              saveStatus: "conflict",
              saveError: undefined,
              conflict: {
                disk,
                message: disk
                  ? "This file was modified outside CrypTex."
                  : "This file was removed or became inaccessible outside CrypTex.",
                comparing: document.conflict?.comparing ?? false,
              },
            },
          };
        }
        const revision = document.revision + 1;
        return {
          ...current,
          [path]: {
            ...document,
            fingerprint: disk.fingerprint,
            state: makeEditorState(path, disk.text),
            revision,
            savedRevision: revision,
            saveStatus: "clean",
            saveError: undefined,
            generation: document.generation + 1,
            conflict: undefined,
          },
        };
      });
    },
    [client, makeEditorState, project],
  );

  useEffect(() => {
    if (!project) return;
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void client
      .onProjectFileChange((change) => {
        if (disposed || change.projectId !== project.projectId) return;
        void client
          .projectIndex(project.projectId)
          .then((index) => {
            if (!disposed) setProjectIndex(index);
          })
          .catch((reason: unknown) => {
            if (!disposed) setError(errorMessage(reason));
          });
        if (change.selfWrite) return;
        projectEpochRef.current += 1;
        void client
          .detectRootDocuments(project.projectId)
          .then(setRootDocuments)
          .catch((reason: unknown) => setError(errorMessage(reason)));
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
                loadedDirectories.map((path, index) => [path, pages[index]!]),
              ),
            );
          })
          .catch((reason: unknown) => {
            if (!disposed) setError(errorMessage(reason));
          });
        const affected =
          change.kind === "rescan"
            ? Object.keys(documentsRef.current)
            : change.relativePaths;
        for (const path of affected) void reconcileExternalDocument(path);
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
  }, [client, project, reconcileExternalDocument]);

  useEffect(() => {
    if (!project) return;
    let disposed = false;
    const unsubscribers: Array<() => void> = [];
    void Promise.all([
      client.onBuildState((state) => {
        if (disposed || state.projectId !== project.projectId) return;
        pdfRequestRef.current += 1;
        synctexRequestRef.current += 1;
        setBuildState((current) => {
          if (current?.operationId !== state.operationId) setBuildLog("");
          buildOperationRef.current = state.operationId;
          if (isTerminalBuild(state)) {
            setDiagnosticEpoch(
              buildEpochsRef.current.get(state.operationId) ??
                projectEpochRef.current,
            );
          }
          return state;
        });
      }),
      client.onBuildOutput((output: BuildOutput) => {
        if (
          disposed ||
          output.projectId !== project.projectId ||
          output.operationId !== buildOperationRef.current
        )
          return;
        setBuildLog((current) => (current + output.text).slice(-200_000));
      }),
    ])
      .then((stops) => {
        if (disposed) stops.forEach((stop) => stop());
        else unsubscribers.push(...stops);
      })
      .catch((reason: unknown) => {
        if (!disposed) setError(errorMessage(reason));
      });
    return () => {
      disposed = true;
      unsubscribers.forEach((stop) => stop());
    };
  }, [client, project]);

  useEffect(() => {
    if (
      !project ||
      !buildState ||
      buildState.phase !== "succeeded" ||
      !buildState.pdfAvailable
    )
      return;
    const request = ++pdfRequestRef.current;
    setPdfError(null);
    void client
      .readBuildPdf(project.projectId, buildState.operationId)
      .then((data) => {
        if (pdfRequestRef.current === request)
          setPdfPreview({
            projectId: project.projectId,
            operationId: buildState.operationId,
            data,
          });
        if (pdfRequestRef.current === request) {
          setSynctexTarget(null);
          setSynctexMessage(null);
        }
      })
      .catch((reason: unknown) => {
        if (pdfRequestRef.current === request)
          setPdfError(errorMessage(reason));
      });
    return () => {
      if (pdfRequestRef.current === request) pdfRequestRef.current += 1;
    };
  }, [buildState, client, project]);

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
    if (!project) return;
    for (const [path, document] of Object.entries(documents)) {
      const scheduled = recoveryTimers.current.get(path);
      if (document.revision <= document.savedRevision) {
        if (scheduled) window.clearTimeout(scheduled.timer);
        recoveryTimers.current.delete(path);
        continue;
      }
      if (scheduled?.revision === document.revision) continue;
      if (scheduled) window.clearTimeout(scheduled.timer);
      const revision = document.revision;
      const text = document.state.doc.toString();
      const fingerprint = document.fingerprint;
      const timer = window.setTimeout(() => {
        recoveryTimers.current.delete(path);
        void client
          .storeRecoverySnapshot(
            project.projectId,
            path,
            text,
            fingerprint,
            revision,
          )
          .then(async (snapshot) => {
            const currentDocument = documentsRef.current[path];
            if (
              !currentDocument ||
              currentDocument.revision !== revision ||
              currentDocument.revision <= currentDocument.savedRevision
            ) {
              await client.deleteRecoverySnapshot(project.projectId, path);
              return;
            }
            setRecoveryInventory((current) => {
              const snapshots = [
                ...(current?.snapshots.filter(
                  (candidate) => candidate.relativePath !== path,
                ) ?? []),
                snapshot,
              ].sort((left, right) =>
                left.updatedAtMs < right.updatedAtMs ? 1 : -1,
              );
              return {
                apiVersion: snapshot.apiVersion,
                snapshots,
                warnings: current?.warnings ?? [],
              };
            });
          })
          .catch((reason: unknown) => setError(errorMessage(reason)));
      }, 250);
      recoveryTimers.current.set(path, { revision, timer });
    }
    for (const [path, scheduled] of recoveryTimers.current) {
      if (!documents[path]) {
        window.clearTimeout(scheduled.timer);
        recoveryTimers.current.delete(path);
      }
    }
  }, [client, documents, project]);
  useEffect(() => {
    const timers = autosaveTimers.current;
    const recovery = recoveryTimers.current;
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (
        Object.values(documentsRef.current).some(
          (document) =>
            document.revision > document.savedRevision ||
            document.saveStatus === "conflict",
        )
      )
        event.preventDefault();
    };
    window.addEventListener("beforeunload", beforeUnload);
    return () => {
      window.removeEventListener("beforeunload", beforeUnload);
      for (const scheduled of timers.values())
        window.clearTimeout(scheduled.timer);
      for (const scheduled of recovery.values())
        window.clearTimeout(scheduled.timer);
    };
  }, []);

  const searchCatalog = useCallback(
    (query: string, context: CommandContext | null) =>
      project
        ? client.searchCatalog(project.projectId, query, context, 50)
        : Promise.resolve([]),
    [client, project],
  );

  const acknowledgeCatalogInsertion = useCallback((request: number) => {
    setEditorInsertion((current) =>
      current?.request === request ? null : current,
    );
  }, []);

  function insertCatalogCommand(hit: CatalogSearchHit) {
    if (!activePath || !documentsRef.current[activePath]) return;
    setEditorInsertion({
      path: activePath,
      request: ++insertionRequestRef.current,
      snippet: hit.entry.snippet,
    });
    setFinderOpen(false);
  }

  async function openProject() {
    if (
      Object.values(documents).some(
        (document) =>
          document.revision > document.savedRevision ||
          document.saveStatus === "conflict",
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
      const [root, detectedRoots, recoveries, index] = await Promise.all([
        client.listDirectory(opened.projectId, ""),
        client.detectRootDocuments(opened.projectId),
        client.listRecoverySnapshots(opened.projectId),
        client.projectIndex(opened.projectId),
      ]);
      setProject(opened);
      setProjectIndex(index);
      setDirectories({ "": root });
      setRootDocuments(detectedRoots);
      setRecoveryInventory(recoveries);
      setReviewingRecovery(null);
      setExpanded(new Set([""]));
      setDocuments({});
      setActivePath(null);
      setEditorNavigation(null);
      setEditorInsertion(null);
      setFinderOpen(false);
      setBuildState(null);
      buildOperationRef.current = null;
      buildEpochsRef.current.clear();
      projectEpochRef.current = 0;
      setDiagnosticEpoch(null);
      setRawBuildLog(null);
      pdfRequestRef.current += 1;
      setPdfPreview(null);
      setPdfError(null);
      synctexRequestRef.current += 1;
      setSynctexTarget(null);
      setSynctexMessage(null);
      setBuildLog("");
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function reviewRecovery(snapshot: RecoverySnapshot) {
    if (!project) return;
    setReviewingRecovery(snapshot);
    setRecoveryDisk(null);
    try {
      setRecoveryDisk(
        await client.readTextFile(project.projectId, snapshot.relativePath),
      );
    } catch (reason) {
      setError(`Recovery source is unavailable: ${errorMessage(reason)}`);
    }
  }

  function restoreRecovery(snapshot: RecoverySnapshot) {
    if (!project || !recoveryDisk) return;
    const current = documentsRef.current[snapshot.relativePath];
    if (
      current &&
      (current.revision > current.savedRevision || current.conflict) &&
      !window.confirm(
        `Replace the current unsaved buffer for ${snapshot.relativePath} with the reviewed recovery copy?`,
      )
    )
      return;
    const revision = (current?.revision ?? 0) + 1;
    setDocuments((documents) => ({
      ...documents,
      [snapshot.relativePath]: {
        path: snapshot.relativePath,
        fingerprint: recoveryDisk.fingerprint,
        state: makeEditorState(snapshot.relativePath, snapshot.text),
        revision,
        savedRevision: revision - 1,
        saveStatus: "dirty",
        saveError: undefined,
        generation: (current?.generation ?? 0) + 1,
        conflict: undefined,
      },
    }));
    setActivePath(snapshot.relativePath);
    setReviewingRecovery(null);
    setRecoveryDisk(null);
  }

  async function discardRecovery(snapshot: RecoverySnapshot) {
    if (!project) return;
    try {
      await client.deleteRecoverySnapshot(
        project.projectId,
        snapshot.relativePath,
      );
      setRecoveryInventory((current) =>
        current
          ? {
              ...current,
              snapshots: current.snapshots.filter(
                (candidate) => candidate.relativePath !== snapshot.relativePath,
              ),
            }
          : current,
      );
      setReviewingRecovery(null);
      setRecoveryDisk(null);
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }
  async function selectRootDocument(relativePath: string) {
    if (!project || !relativePath) return;
    try {
      const selected = await client.setRootDocument(
        project.projectId,
        relativePath,
      );
      setRootDocuments(selected);
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }
  async function requestBuild() {
    if (!project || !rootDocuments?.selected) return;
    if (
      Object.values(documentsRef.current).some(
        (document) => document.saveStatus === "conflict",
      )
    ) {
      setError("Resolve external file conflicts before compiling.");
      return;
    }
    setError(null);
    pdfRequestRef.current += 1;
    try {
      await Promise.all(
        Object.keys(documentsRef.current).map((path) => saveDocument(path)),
      );
      const epoch = projectEpochRef.current;
      const state = await client.requestBuild(project.projectId, "explicit");
      buildOperationRef.current = state.operationId;
      buildEpochsRef.current.set(state.operationId, epoch);
      setDiagnosticEpoch(epoch);
      setRawBuildLog(null);
      setBuildState(state);
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  async function cancelBuild() {
    if (!project || !buildState) return;
    try {
      const cancelled = await client.cancelBuild(
        project.projectId,
        buildState.operationId,
      );
      if (cancelled && buildState.phase === "queued")
        setBuildState({ ...buildState, phase: "cancelled" });
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  async function cleanBuildArtifacts() {
    if (!project) return;
    try {
      await client.cleanBuildArtifacts(project.projectId);
      setBuildState(null);
      buildOperationRef.current = null;
      buildEpochsRef.current.clear();
      setDiagnosticEpoch(null);
      setRawBuildLog(null);
      pdfRequestRef.current += 1;
      setPdfPreview(null);
      setPdfError(null);
      synctexRequestRef.current += 1;
      setSynctexTarget(null);
      setSynctexMessage(null);
      setBuildLog("");
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  async function loadRawBuildLog() {
    if (!project || !buildState?.rawLogAvailable) return;
    try {
      const log = await client.readBuildLog(
        project.projectId,
        buildState.operationId,
      );
      setRawBuildLog(
        log.text + (log.truncated ? "\n\n[Log truncated by CrypTex]" : ""),
      );
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  async function forwardSearch() {
    if (!project || !activePath || !pdfPreview) return;
    const document = documentsRef.current[activePath];
    if (!document) return;
    const cursor = document.state.selection.main.head;
    const sourceLine = document.state.doc.lineAt(cursor);
    const request = ++synctexRequestRef.current;
    setSynctexMessage("Locating source position in PDF…");
    try {
      const target = await client.forwardSynctex(
        project.projectId,
        pdfPreview.operationId,
        activePath,
        sourceLine.number,
        cursor - sourceLine.from + 1,
      );
      if (synctexRequestRef.current !== request) return;
      if (!target) {
        setSynctexTarget(null);
        setSynctexMessage(
          "No SyncTeX position was found for this source line.",
        );
        return;
      }
      setSynctexTarget({ ...target, requestId: request });
      setSynctexMessage(null);
    } catch (reason) {
      if (synctexRequestRef.current === request) {
        setSynctexTarget(null);
        setSynctexMessage(errorMessage(reason));
      }
    }
  }

  async function inverseSearch(page: number, x: number, y: number) {
    if (!project || !pdfPreview) return;
    const operationId = pdfPreview.operationId;
    const request = ++synctexRequestRef.current;
    setSynctexMessage("Locating PDF position in source…");
    try {
      const source = await client.inverseSynctex(
        project.projectId,
        operationId,
        page,
        x,
        y,
      );
      if (synctexRequestRef.current !== request) return;
      if (!source) {
        setSynctexMessage(
          "No SyncTeX source position was found for this PDF point.",
        );
        return;
      }
      await navigateToSource(source.relativePath, source.line, source.column);
      if (synctexRequestRef.current === request) setSynctexMessage(null);
    } catch (reason) {
      if (synctexRequestRef.current === request)
        setSynctexMessage(errorMessage(reason));
    }
  }

  async function navigateToDiagnostic(diagnostic: Diagnostic) {
    if (!diagnostic.source) return;
    await navigateToSource(
      diagnostic.source.relativePath,
      diagnostic.source.startLine,
    );
  }

  async function navigateToSource(
    path: string,
    line: number,
    column: number | null = null,
  ) {
    if (!project) return;
    const navigate = () => {
      setActivePath(path);
      setEditorNavigation((navigation) => ({
        path,
        line,
        column,
        request: (navigation?.request ?? 0) + 1,
      }));
    };
    if (documentsRef.current[path]) {
      navigate();
      return;
    }
    try {
      const loaded = await client.readTextFile(project.projectId, path);
      const state = makeEditorState(path, loaded.text);
      setDocuments((documents) => ({
        ...documents,
        [path]: {
          path,
          fingerprint: loaded.fingerprint,
          state,
          revision: 0,
          savedRevision: 0,
          saveStatus: "clean",
          saveError: undefined,
          generation: 0,
          conflict: undefined,
        },
      }));
      navigate();
    } catch (reason) {
      setError(errorMessage(reason));
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
    setEditorNavigation(null);
    if (documents[path]) {
      setActivePath(path);
      return;
    }
    try {
      const loaded = await client.readTextFile(project.projectId, path);
      const state = makeEditorState(path, loaded.text);
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
          generation: 0,
          conflict: undefined,
        },
      }));
      setActivePath(path);
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  async function reloadDocumentFromDisk(path: string) {
    if (!project) return;
    try {
      const disk = await client.readTextFile(project.projectId, path);
      setDocuments((current) => {
        const document = current[path];
        if (!document) return current;
        const revision = document.revision + 1;
        return {
          ...current,
          [path]: {
            ...document,
            fingerprint: disk.fingerprint,
            state: makeEditorState(path, disk.text),
            revision,
            savedRevision: revision,
            saveStatus: "clean",
            saveError: undefined,
            generation: document.generation + 1,
            conflict: undefined,
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
                saveStatus: "conflict",
                conflict: {
                  disk: undefined,
                  message: errorMessage(reason),
                  comparing: false,
                },
              },
            }
          : current,
      );
    }
  }

  async function overwriteDiskWithDocument(path: string) {
    if (!project) return;
    const snapshot = documentsRef.current[path];
    if (!snapshot?.conflict) return;
    try {
      const fresh = await client.readTextFile(project.projectId, path);
      const result = await client.writeTextFile(
        project.projectId,
        path,
        snapshot.state.doc.toString(),
        fresh.fingerprint,
      );
      setDocuments((current) => {
        const document = current[path];
        if (!document) return current;
        const savedRevision = snapshot.revision;
        return {
          ...current,
          [path]: {
            ...document,
            fingerprint: result.fingerprint,
            savedRevision,
            saveStatus: document.revision > savedRevision ? "dirty" : "clean",
            saveError: undefined,
            conflict: undefined,
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
                saveStatus: "conflict",
                conflict: {
                  disk: current[path].conflict?.disk,
                  message: `Overwrite aborted: ${errorMessage(reason)}`,
                  comparing: current[path].conflict?.comparing ?? false,
                },
              },
            }
          : current,
      );
    }
  }

  function toggleComparison(path: string) {
    setDocuments((current) => {
      const document = current[path];
      if (!document?.conflict) return current;
      return {
        ...current,
        [path]: {
          ...document,
          conflict: {
            ...document.conflict,
            comparing: !document.conflict.comparing,
          },
        },
      };
    });
  }
  function closeDocument(path: string) {
    const document = documents[path];
    if (editorNavigation?.path === path) setEditorNavigation(null);
    if (
      document &&
      (document.revision > document.savedRevision ||
        document.saveStatus === "conflict") &&
      !window.confirm(`Discard unsaved changes to ${path}?`)
    )
      return;
    if (project) {
      void client
        .deleteRecoverySnapshot(project.projectId, path)
        .then(() =>
          setRecoveryInventory((current) =>
            current
              ? {
                  ...current,
                  snapshots: current.snapshots.filter(
                    (snapshot) => snapshot.relativePath !== path,
                  ),
                }
              : current,
          ),
        )
        .catch((reason: unknown) => setError(errorMessage(reason)));
    }
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
              {rootDocuments ? (
                <div className="root-selection">
                  <label htmlFor="root-document">Root document</label>
                  {rootDocuments.candidates.length ? (
                    <select
                      id="root-document"
                      value={rootDocuments.selected ?? ""}
                      onChange={(event) =>
                        void selectRootDocument(event.target.value)
                      }
                    >
                      <option value="">Choose a root…</option>
                      {rootDocuments.candidates.map((candidate) => (
                        <option
                          key={candidate.relativePath}
                          value={candidate.relativePath}
                        >
                          {candidate.relativePath}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <span>No root document detected</span>
                  )}
                  {rootDocuments.candidates.length > 1 &&
                  !rootDocuments.selected ? (
                    <p role="status">
                      Multiple root documents were found. Choose one before
                      building.
                    </p>
                  ) : null}
                </div>
              ) : null}
              <section className="build-panel" aria-labelledby="build-title">
                <div className="build-heading">
                  <h3 id="build-title">Build</h3>
                  <div className="build-actions">
                    <button
                      type="button"
                      onClick={() => void requestBuild()}
                      disabled={
                        !rootDocuments?.selected ||
                        buildState?.phase === "queued" ||
                        buildState?.phase === "running"
                      }
                    >
                      Compile
                    </button>
                    {buildState?.phase === "queued" ||
                    buildState?.phase === "running" ? (
                      <button type="button" onClick={() => void cancelBuild()}>
                        Cancel
                      </button>
                    ) : (
                      <button
                        type="button"
                        onClick={() => void cleanBuildArtifacts()}
                      >
                        Clean
                      </button>
                    )}
                  </div>
                </div>
                {buildState ? (
                  <div className="build-status" role="status">
                    <strong>{buildLabel(buildState)}</strong>
                    <span>
                      {buildState.rootDocument} · {buildState.engine}
                    </span>
                    {buildState.message ? (
                      <span>{buildState.message}</span>
                    ) : null}
                    {buildState.lastSuccessfulOperationId ? (
                      <span>Last successful PDF is available.</span>
                    ) : null}
                  </div>
                ) : (
                  <p>No build has run in this session.</p>
                )}
                {buildLog ? (
                  <details>
                    <summary>Build log</summary>
                    <pre className="build-log">{buildLog}</pre>
                  </details>
                ) : null}
              </section>
              {buildState && isTerminalBuild(buildState) ? (
                <section
                  className="problems-panel"
                  aria-labelledby="problems-title"
                >
                  <div className="problems-heading">
                    <h3 id="problems-title">Problems</h3>
                    <select
                      aria-label="Diagnostic severity"
                      value={diagnosticFilter}
                      onChange={(event) =>
                        setDiagnosticFilter(
                          event.target.value as DiagnosticSeverity | "all",
                        )
                      }
                    >
                      <option value="all">All</option>
                      <option value="error">Errors</option>
                      <option value="warning">Warnings</option>
                      <option value="information">Information</option>
                    </select>
                  </div>
                  {diagnosticsStale ? (
                    <p className="stale-diagnostics" role="status">
                      Diagnostics are from an older document version.
                    </p>
                  ) : null}
                  {visibleDiagnostics.length ? (
                    <ul className="problem-list">
                      {visibleDiagnostics.map((diagnostic, index) => (
                        <li key={diagnosticKey(diagnostic, index)}>
                          <button
                            type="button"
                            className={"problem problem-" + diagnostic.severity}
                            disabled={!diagnostic.source}
                            onClick={() =>
                              void navigateToDiagnostic(diagnostic)
                            }
                          >
                            <strong>{diagnostic.code}</strong>
                            <span>{diagnostic.message}</span>
                            <small>
                              {diagnostic.source
                                ? diagnostic.source.relativePath +
                                  ":" +
                                  diagnostic.source.startLine
                                : diagnostic.phase}
                            </small>
                          </button>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p>No diagnostics match this filter.</p>
                  )}
                  {buildState.rawLogAvailable ? (
                    <button
                      type="button"
                      onClick={() => void loadRawBuildLog()}
                    >
                      Open raw LaTeX log
                    </button>
                  ) : null}
                  {rawBuildLog !== null ? (
                    <details open>
                      <summary>Raw LaTeX log</summary>
                      <pre className="build-log">{rawBuildLog}</pre>
                    </details>
                  ) : null}
                </section>
              ) : null}
              {recoveryInventory &&
              (recoveryInventory.snapshots.length > 0 ||
                recoveryInventory.warnings.length > 0) ? (
                <section
                  className="recovery-list"
                  aria-labelledby="recovery-title"
                >
                  <h3 id="recovery-title">Recovery</h3>
                  {recoveryInventory.warnings.map((warning, index) => (
                    <p role="alert" key={`${warning}:${index}`}>
                      {warning}
                    </p>
                  ))}
                  {recoveryInventory.snapshots.map((snapshot) => (
                    <button
                      type="button"
                      key={snapshot.relativePath}
                      onClick={() => void reviewRecovery(snapshot)}
                    >
                      Review {snapshot.relativePath}
                    </button>
                  ))}
                </section>
              ) : null}
              {projectIndex ? (
                <ProjectOutline
                  index={projectIndex}
                  onNavigate={(path, record) =>
                    void navigateToSource(
                      path,
                      record.range.startLine,
                      record.range.startColumn,
                    )
                  }
                />
              ) : null}
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
          {reviewingRecovery ? (
            <section
              className="recovery-review"
              aria-labelledby="recovery-review-title"
            >
              <div className="recovery-review-heading">
                <strong id="recovery-review-title">
                  Review recovery: {reviewingRecovery.relativePath}
                </strong>
                <button
                  type="button"
                  aria-label="Close recovery review"
                  onClick={() => {
                    setReviewingRecovery(null);
                    setRecoveryDisk(null);
                  }}
                >
                  ×
                </button>
              </div>
              <div className="recovery-comparison">
                <section>
                  <h2>Recovered buffer</h2>
                  <pre>{reviewingRecovery.text}</pre>
                </section>
                <section>
                  <h2>Current disk version</h2>
                  <pre>{recoveryDisk?.text ?? "File is unavailable."}</pre>
                </section>
              </div>
              <div className="recovery-actions">
                <button
                  type="button"
                  disabled={!recoveryDisk}
                  onClick={() => restoreRecovery(reviewingRecovery)}
                >
                  Restore reviewed buffer
                </button>
                <button
                  type="button"
                  onClick={() => void discardRecovery(reviewingRecovery)}
                >
                  Discard recovery copy
                </button>
              </div>
            </section>
          ) : null}
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
                  {document.saveStatus === "conflict"
                    ? " !"
                    : document.revision > document.savedRevision
                      ? " •"
                      : ""}
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
                title="Command Finder (Ctrl/Cmd+K)"
                onClick={() => setFinderOpen(true)}
              >
                Command Finder
              </button>
              <button
                type="button"
                onClick={() => void saveDocument(activePath)}
                disabled={
                  documents[activePath].saveStatus === "saving" ||
                  documents[activePath].saveStatus === "conflict"
                }
              >
                Save
              </button>
              <button
                type="button"
                onClick={() => void forwardSearch()}
                disabled={!pdfPreview}
              >
                Show in PDF
              </button>
              <span role="status">
                {documents[activePath].saveStatus === "error"
                  ? documents[activePath].saveError
                  : documents[activePath].saveStatus}
              </span>
            </div>
          ) : null}
          {activePath && documents[activePath]?.conflict ? (
            <section className="conflict-banner" role="alert">
              <div>
                <strong>External change detected</strong>
                <p>{documents[activePath].conflict.message}</p>
              </div>
              <div className="conflict-actions">
                <button
                  type="button"
                  onClick={() => toggleComparison(activePath)}
                >
                  {documents[activePath].conflict.comparing
                    ? "Hide comparison"
                    : "Compare"}
                </button>
                <button
                  type="button"
                  onClick={() => void reloadDocumentFromDisk(activePath)}
                  disabled={!documents[activePath].conflict.disk}
                >
                  Reload disk version
                </button>
                <button
                  type="button"
                  onClick={() => void overwriteDiskWithDocument(activePath)}
                  disabled={!documents[activePath].conflict.disk}
                >
                  Overwrite with my version
                </button>
              </div>
              {documents[activePath].conflict.comparing ? (
                <div className="conflict-comparison">
                  <section>
                    <h2>Your buffer</h2>
                    <pre>{documents[activePath].state.doc.toString()}</pre>
                  </section>
                  <section>
                    <h2>Disk version</h2>
                    <pre>
                      {documents[activePath].conflict.disk?.text ??
                        "File is unavailable."}
                    </pre>
                  </section>
                </div>
              ) : null}
            </section>
          ) : null}
          {activePath && documents[activePath] ? (
            <CodeEditor
              key={`${activePath}:${documents[activePath].generation}`}
              state={documents[activePath].state}
              diagnostics={activeDiagnosticMarkers}
              navigation={
                editorNavigation?.path === activePath
                  ? editorNavigation
                  : undefined
              }
              insertion={
                editorInsertion?.path === activePath
                  ? editorInsertion
                  : undefined
              }
              onInsertionApplied={acknowledgeCatalogInsertion}
            />
          ) : (
            <p>Select a text file to begin editing.</p>
          )}
        </section>
        <section className="pane pdf-pane" aria-labelledby="pane-pdf">
          <h1 id="pane-pdf">PDF</h1>
          {pdfError ? <p role="alert">{pdfError}</p> : null}
          {synctexMessage ? <p role="status">{synctexMessage}</p> : null}
          {pdfPreview && pdfPreview.projectId === project?.projectId ? (
            <PdfViewer
              key={pdfPreview.projectId}
              projectId={pdfPreview.projectId}
              operationId={pdfPreview.operationId}
              data={pdfPreview.data}
              forwardTarget={synctexTarget ?? undefined}
              onInverseSearch={(page, x, y) => void inverseSearch(page, x, y)}
            />
          ) : (
            <p>A successful build will appear here.</p>
          )}
        </section>
      </div>
      {finderOpen && project && activePath ? (
        <CommandFinder
          onClose={() => setFinderOpen(false)}
          onInsert={insertCatalogCommand}
          search={searchCatalog}
        />
      ) : null}
    </main>
  );
}

function ProjectOutline({
  index,
  onNavigate,
}: {
  index: ProjectIndex;
  onNavigate: (path: string, record: IndexRecord) => void;
}) {
  const labels = new Set(
    index.files.flatMap((file) =>
      file.records
        .filter((record) => record.kind === "label")
        .map((record) => record.name),
    ),
  );
  const entries = index.files.flatMap((file) =>
    file.records
      .filter((record) =>
        [
          "section",
          "label",
          "reference",
          "citation",
          "environment",
          "macroDefinition",
          "cryptocode",
        ].includes(record.kind),
      )
      .map((record) => ({ file: file.relativePath, record })),
  );
  return (
    <details className="project-outline" open>
      <summary>Outline ({entries.length})</summary>
      {entries.length ? (
        <ul>
          {entries.map(({ file, record }, index) => {
            const missing =
              record.kind === "reference" && !labels.has(record.name);
            return (
              <li key={file + ":" + record.range.startByte + ":" + index}>
                <button type="button" onClick={() => onNavigate(file, record)}>
                  <span>{outlineKind(record.kind)}</span>
                  <strong>{record.name}</strong>
                  <small>
                    {file}:{record.range.startLine}
                    {missing ? " � missing target" : ""}
                  </small>
                </button>
              </li>
            );
          })}
        </ul>
      ) : (
        <p>No indexed symbols.</p>
      )}
      {index.issues.length ? (
        <p role="status">{index.issues.length} index issue(s).</p>
      ) : null}
    </details>
  );
}

function outlineKind(kind: IndexRecord["kind"]): string {
  return kind.replace(/([A-Z])/g, " $1").toLowerCase();
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

function errorCode(reason: unknown): string | undefined {
  return typeof reason === "object" &&
    reason &&
    "code" in reason &&
    typeof reason.code === "string"
    ? reason.code
    : undefined;
}
function isTerminalBuild(state: BuildState): boolean {
  return !["queued", "running"].includes(state.phase);
}

function diagnosticKey(diagnostic: Diagnostic, index: number): string {
  return [
    diagnostic.code,
    diagnostic.source?.relativePath ?? "unlocated",
    diagnostic.source?.startLine ?? 0,
    index,
  ].join(":");
}

function buildLabel(state: BuildState): string {
  switch (state.phase) {
    case "queued":
      return "Build queued";
    case "running":
      return "Building…";
    case "succeeded":
      return state.elapsedMs === null
        ? "Build succeeded"
        : `Build succeeded in ${Number(state.elapsedMs) / 1000}s`;
    case "failed":
      return state.exitCode === null
        ? "Build failed"
        : `Build failed (exit ${state.exitCode})`;
    case "cancelled":
      return "Build cancelled";
    case "timedOut":
      return "Build timed out";
  }
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
