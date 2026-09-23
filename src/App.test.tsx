import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { BackendClient } from "./api/client";
import type { BuildState } from "./bindings/BuildState";
import type { ProjectFileChange } from "./bindings/ProjectFileChange";

vi.mock("./pdf/PdfViewer", () => ({
  PdfViewer: ({
    operationId,
    forwardTarget,
    onInverseSearch,
  }: {
    operationId: string;
    forwardTarget?: { page: number };
    onInverseSearch?: (page: number, x: number, y: number) => void;
  }) => (
    <div data-testid="pdf-preview">
      {operationId}
      {forwardTarget ? " · page " + forwardTarget.page : ""}
      {onInverseSearch ? (
        <button type="button" onClick={() => onInverseSearch(2, 30, 40)}>
          Open PDF position
        </button>
      ) : null}
    </div>
  ),
}));

describe("App", () => {
  it("renders the minimal Project, Editor, and PDF workspace", () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: "Project" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Editor" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "PDF" })).toBeInTheDocument();
  });

  it("opens a selected folder and renders its bounded root listing", async () => {
    const openProject = vi.fn().mockResolvedValue({
      apiVersion: 1,
      projectId: "a".repeat(64),
      name: "paper",
      canonicalRoot: "/paper",
    });
    const listDirectory = vi.fn().mockResolvedValue({
      apiVersion: 1,
      directory: "",
      truncated: false,
      entries: [
        {
          name: "main.tex",
          relativePath: "main.tex",
          kind: "file",
          isSymlink: false,
          accessible: true,
          hidden: false,
          generated: false,
        },
      ],
    });
    const readTextFile = vi.fn().mockResolvedValue({
      apiVersion: 1,
      relativePath: "main.tex",
      text: "\\\\documentclass{article}",
      fingerprint: "f".repeat(64),
      sizeBytes: 23,
    });
    const writeTextFile = vi.fn().mockResolvedValue({
      apiVersion: 1,
      relativePath: "main.tex",
      fingerprint: "e".repeat(64),
      sizeBytes: 7,
    });
    const client: BackendClient = {
      health: vi.fn(),
      toolchainReadiness: vi.fn(),
      openProject,
      listDirectory,
      readTextFile,
      writeTextFile,
      detectRootDocuments: vi
        .fn()
        .mockResolvedValue(rootCandidates("a".repeat(64))),
      setRootDocument: vi
        .fn()
        .mockResolvedValue(rootCandidates("a".repeat(64))),
      ...recoveryMocks(),
      ...trustMocks(),
      ...buildMocks(),
      onProjectFileChange: vi.fn().mockResolvedValue(() => undefined),
    };
    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    await screen.findByRole("button", { name: /main\.tex/ });
    expect(openProject).toHaveBeenCalledWith("/paper");
    expect(listDirectory).toHaveBeenCalledWith("a".repeat(64), "");
    fireEvent.click(screen.getByRole("button", { name: /main\.tex/ }));
    await waitFor(() =>
      expect(readTextFile).toHaveBeenCalledWith("a".repeat(64), "main.tex"),
    );
    expect(screen.getByRole("tab", { name: "main.tex" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    const content = await waitFor(() => {
      const element = document.querySelector<HTMLElement>(".cm-content");
      expect(element).not.toBeNull();
      return element;
    });
    expect(content).not.toBeNull();
    if (!content) return;
    content.textContent = "changed";
    fireEvent.input(content, { inputType: "insertText", data: "changed" });
    await waitFor(() =>
      expect(
        screen.getByRole("tab", { name: /main\.tex •/ }),
      ).toBeInTheDocument(),
    );
    await waitFor(() => expect(writeTextFile).toHaveBeenCalled(), {
      timeout: 1_500,
    });
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent("clean"),
    );
  });

  it("renders indexed navigation candidates and opens the selected source range", async () => {
    const projectId = "6".repeat(64);
    const readTextFile = vi
      .fn()
      .mockImplementation((_id: string, path: string) =>
        Promise.resolve({
          apiVersion: 1,
          relativePath: path,
          text: "first\nsecond",
          fingerprint: "5".repeat(64),
          sizeBytes: 12,
        }),
      );
    const client = conflictClient(projectId, readTextFile, () => undefined);
    const range = {
      startByte: 0n,
      endByte: 10n,
      startLine: 2,
      startColumn: 1,
      endLine: 2,
      endColumn: 8,
    };
    client.projectIndex = vi.fn().mockResolvedValue({
      apiVersion: 1,
      schemaVersion: 1,
      projectId,
      generation: 2n,
      completeness: "bestEffort",
      scannerLimits: {
        maxProjectFiles: 10000,
        maxTotalBytes: 268435456n,
        maxFileBytes: 5242880n,
        maxRecordsPerFile: 50000,
        maxBraceDepth: 256,
        maxCommandBytes: 4096,
      },
      files: [
        {
          relativePath: "main.tex",
          fingerprint: "5".repeat(64),
          status: "complete",
          records: [
            {
              kind: "label",
              name: "shared",
              target: null,
              range,
              confidence: "exact",
              provenance: "lexical",
            },
            {
              kind: "reference",
              name: "missing",
              target: null,
              range,
              confidence: "exact",
              provenance: "lexical",
            },
          ],
          issues: [],
        },
        {
          relativePath: "sections/proof.tex",
          fingerprint: "4".repeat(64),
          status: "complete",
          records: [
            {
              kind: "label",
              name: "shared",
              target: null,
              range,
              confidence: "exact",
              provenance: "lexical",
            },
          ],
          issues: [],
        },
      ],
      issues: [],
    });
    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    const candidates = await screen.findAllByRole("button", { name: /shared/ });
    expect(candidates).toHaveLength(2);
    expect(
      screen.getByRole("button", { name: /missing target/ }),
    ).toBeVisible();
    fireEvent.click(candidates[1]!);
    expect(
      await screen.findByRole("tab", { name: "proof.tex" }),
    ).toHaveAttribute("aria-selected", "true");
    expect(readTextFile).toHaveBeenCalledWith(projectId, "sections/proof.tex");
  });

  it("reloads a clean open buffer after an external modification", async () => {
    let notify: ((change: ProjectFileChange) => void) | undefined;
    const projectId = "b".repeat(64);
    const readTextFile = vi
      .fn()
      .mockResolvedValueOnce({
        apiVersion: 1,
        relativePath: "main.tex",
        text: "original",
        fingerprint: "1".repeat(64),
        sizeBytes: 8,
      })
      .mockResolvedValue({
        apiVersion: 1,
        relativePath: "main.tex",
        text: "external",
        fingerprint: "2".repeat(64),
        sizeBytes: 8,
      });
    const client = conflictClient(projectId, readTextFile, (listener) => {
      notify = listener;
    });
    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    await screen.findByRole("button", { name: /main\.tex/ });
    fireEvent.click(screen.getByRole("button", { name: /main\.tex/ }));
    await waitFor(() =>
      expect(document.querySelector(".cm-content")).toHaveTextContent(
        "original",
      ),
    );

    act(() =>
      notify?.({
        apiVersion: 1,
        projectId,
        relativePaths: ["main.tex"],
        kind: "modify",
        selfWrite: false,
      }),
    );

    await waitFor(() =>
      expect(document.querySelector(".cm-content")).toHaveTextContent(
        "external",
      ),
    );
    expect(screen.queryByText("External change detected")).toBeNull();
  });

  it("blocks a dirty buffer until an external change is explicitly resolved", async () => {
    let notify: ((change: ProjectFileChange) => void) | undefined;
    const projectId = "c".repeat(64);
    const readTextFile = vi
      .fn()
      .mockResolvedValueOnce({
        apiVersion: 1,
        relativePath: "main.tex",
        text: "original",
        fingerprint: "1".repeat(64),
        sizeBytes: 8,
      })
      .mockResolvedValue({
        apiVersion: 1,
        relativePath: "main.tex",
        text: "external",
        fingerprint: "2".repeat(64),
        sizeBytes: 8,
      });
    const client = conflictClient(projectId, readTextFile, (listener) => {
      notify = listener;
    });
    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    await screen.findByRole("button", { name: /main\.tex/ });
    fireEvent.click(screen.getByRole("button", { name: /main\.tex/ }));
    const content = await waitFor(() => {
      const element = document.querySelector<HTMLElement>(".cm-content");
      expect(element).not.toBeNull();
      return element!;
    });
    content.textContent = "my edit";
    fireEvent.input(content, { inputType: "insertText", data: "my edit" });
    await screen.findByRole("tab", { name: /main\.tex •/ });

    act(() =>
      notify?.({
        apiVersion: 1,
        projectId,
        relativePaths: ["main.tex"],
        kind: "modify",
        selfWrite: false,
      }),
    );

    expect(await screen.findByText("External change detected")).toBeVisible();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Compare" }));
    expect(screen.getByText("Your buffer")).toBeVisible();
    expect(screen.getByText("Disk version")).toBeVisible();
    expect(screen.getByText("external")).toBeVisible();
    fireEvent.click(
      screen.getByRole("button", { name: "Overwrite with my version" }),
    );
    await waitFor(() =>
      expect(client.writeTextFile).toHaveBeenCalledWith(
        projectId,
        "main.tex",
        "my edit",
        "2".repeat(64),
      ),
    );
    await waitFor(() =>
      expect(screen.queryByText("External change detected")).toBeNull(),
    );
  });

  it("prompts for an ambiguous root and persists the explicit selection", async () => {
    const projectId = "d".repeat(64);
    const readTextFile = vi.fn();
    const client = conflictClient(projectId, readTextFile, () => undefined);
    const ambiguous = {
      apiVersion: 1,
      candidates: [
        { relativePath: "main.tex", reasons: ["documentClass" as const] },
        { relativePath: "notes.tex", reasons: ["documentClass" as const] },
      ],
      selected: null,
    };
    client.detectRootDocuments = vi.fn().mockResolvedValue(ambiguous);
    client.setRootDocument = vi.fn().mockResolvedValue({
      ...ambiguous,
      selected: "notes.tex",
    });
    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));

    expect(
      await screen.findByText(/Multiple root documents were found/),
    ).toBeVisible();
    fireEvent.change(screen.getByLabelText("Root document"), {
      target: { value: "notes.tex" },
    });

    await waitFor(() =>
      expect(client.setRootDocument).toHaveBeenCalledWith(
        projectId,
        "notes.tex",
      ),
    );
    await waitFor(() =>
      expect(screen.getByLabelText("Root document")).toHaveValue("notes.tex"),
    );
  });

  it("requires review before restoring a snapshot and removes it after save", async () => {
    const projectId = "9".repeat(64);
    const disk = {
      apiVersion: 1,
      relativePath: "main.tex",
      text: "disk version",
      fingerprint: "4".repeat(64),
      sizeBytes: 12,
    };
    const recovered = {
      apiVersion: 1,
      snapshotVersion: 1,
      projectId,
      relativePath: "main.tex",
      text: "recovered draft",
      baseFingerprint: "3".repeat(64),
      revision: 4n,
      updatedAtMs: 10n,
    };
    const client = conflictClient(
      projectId,
      vi.fn().mockResolvedValue(disk),
      () => undefined,
    );
    client.listRecoverySnapshots = vi.fn().mockResolvedValue({
      apiVersion: 1,
      snapshots: [recovered],
      warnings: [],
    });
    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));

    const review = await screen.findByRole("button", {
      name: "Review main.tex",
    });
    expect(screen.queryByText("recovered draft")).toBeNull();
    fireEvent.click(review);
    expect(await screen.findByText("Recovered buffer")).toBeVisible();
    expect(screen.getByText("recovered draft")).toBeVisible();
    expect(screen.getByText("disk version")).toBeVisible();

    fireEvent.click(
      screen.getByRole("button", { name: "Restore reviewed buffer" }),
    );
    await waitFor(() =>
      expect(document.querySelector(".cm-content")).toHaveTextContent(
        "recovered draft",
      ),
    );
    await waitFor(
      () => expect(client.storeRecoverySnapshot).toHaveBeenCalled(),
      {
        timeout: 1_000,
      },
    );
    await waitFor(
      () =>
        expect(client.deleteRecoverySnapshot).toHaveBeenCalledWith(
          projectId,
          "main.tex",
        ),
      { timeout: 1_500 },
    );
  });

  it("keeps the last successful PDF and ignores stale refresh responses", async () => {
    const projectId = "8".repeat(64);
    const client = conflictClient(projectId, vi.fn(), () => undefined);
    let emitBuild: ((state: BuildState) => void) | undefined;
    const onBuildState = vi
      .fn()
      .mockImplementation((listener: (state: BuildState) => void) => {
        emitBuild = listener;
        return Promise.resolve(() => undefined);
      });
    client.onBuildState = onBuildState;
    let resolveFirst: ((data: Uint8Array) => void) | undefined;
    client.readBuildPdf = vi
      .fn()
      .mockImplementation((_projectId, operationId) => {
        if (operationId === "build-00000000000000000001")
          return new Promise<Uint8Array>((resolve) => {
            resolveFirst = resolve;
          });
        return Promise.resolve(new Uint8Array([2]));
      });

    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    await waitFor(() => expect(onBuildState).toHaveBeenCalled());

    act(() => {
      emitBuild?.(
        pdfBuildState(
          projectId,
          "build-00000000000000000001",
          "succeeded",
          true,
        ),
      );
    });
    await waitFor(() =>
      expect(client.readBuildPdf).toHaveBeenCalledWith(
        projectId,
        "build-00000000000000000001",
      ),
    );
    await act(async () => {
      emitBuild?.(
        pdfBuildState(
          projectId,
          "build-00000000000000000002",
          "running",
          false,
        ),
      );
      resolveFirst?.(new Uint8Array([1]));
      await Promise.resolve();
    });
    expect(screen.queryByTestId("pdf-preview")).toBeNull();

    act(() => {
      emitBuild?.(
        pdfBuildState(
          projectId,
          "build-00000000000000000002",
          "succeeded",
          true,
        ),
      );
    });
    expect(await screen.findByTestId("pdf-preview")).toHaveTextContent(
      "build-00000000000000000002",
    );

    act(() => {
      emitBuild?.(
        pdfBuildState(projectId, "build-00000000000000000003", "failed", false),
      );
    });
    expect(screen.getByTestId("pdf-preview")).toHaveTextContent(
      "build-00000000000000000002",
    );
  });

  it("maps the active source cursor to the retained PDF", async () => {
    const projectId = "9".repeat(64);
    const operationId = "build-00000000000000000009";
    const client = conflictClient(
      projectId,
      vi.fn().mockResolvedValue({
        apiVersion: 1,
        relativePath: "main.tex",
        text: "first line\nsecond line",
        fingerprint: "7".repeat(64),
        sizeBytes: 22,
      }),
      () => undefined,
    );
    let emitBuild: ((state: BuildState) => void) | undefined;
    client.onBuildState = vi
      .fn()
      .mockImplementation((listener: (state: BuildState) => void) => {
        emitBuild = listener;
        return Promise.resolve(() => undefined);
      });
    client.readBuildPdf = vi.fn().mockResolvedValue(new Uint8Array([1]));
    client.forwardSynctex = vi.fn().mockResolvedValue({
      apiVersion: 1,
      projectId,
      operationId,
      page: 3,
      x: 72,
      y: 144,
      width: 120,
      height: 12,
    });

    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    const source = await screen.findByRole("button", { name: /main\.tex/ });
    fireEvent.click(source);
    await screen.findByRole("tab", { name: "main.tex" });
    act(() => {
      emitBuild?.(pdfBuildState(projectId, operationId, "succeeded", true));
    });
    await screen.findByTestId("pdf-preview");

    fireEvent.click(screen.getByRole("button", { name: "Show in PDF" }));
    await waitFor(() =>
      expect(client.forwardSynctex).toHaveBeenCalledWith(
        projectId,
        operationId,
        "main.tex",
        1,
        1,
      ),
    );
    expect(await screen.findByTestId("pdf-preview")).toHaveTextContent(
      "page 3",
    );

    vi.mocked(client.forwardSynctex).mockResolvedValueOnce(null);
    fireEvent.click(screen.getByRole("button", { name: "Show in PDF" }));
    expect(
      await screen.findByText(
        "No SyncTeX position was found for this source line.",
      ),
    ).toBeVisible();
  });

  it("maps a retained PDF position back to a validated source file", async () => {
    const projectId = "7".repeat(64);
    const operationId = "build-00000000000000000010";
    const readTextFile = vi
      .fn()
      .mockImplementation((_projectId: string, path: string) =>
        Promise.resolve({
          apiVersion: 1,
          relativePath: path,
          text: "first line\nsecond line",
          fingerprint: "6".repeat(64),
          sizeBytes: 22,
        }),
      );
    const client = conflictClient(projectId, readTextFile, () => undefined);
    let emitBuild: ((state: BuildState) => void) | undefined;
    client.onBuildState = vi
      .fn()
      .mockImplementation((listener: (state: BuildState) => void) => {
        emitBuild = listener;
        return Promise.resolve(() => undefined);
      });
    client.readBuildPdf = vi.fn().mockResolvedValue(new Uint8Array([1]));
    client.inverseSynctex = vi.fn().mockResolvedValue({
      apiVersion: 1,
      projectId,
      operationId,
      relativePath: "sections/proof.tex",
      line: 2,
      column: 3,
    });

    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    await screen.findByRole("button", { name: /main\.tex/ });
    act(() => {
      emitBuild?.(pdfBuildState(projectId, operationId, "succeeded", true));
    });
    await screen.findByTestId("pdf-preview");
    fireEvent.click(screen.getByRole("button", { name: "Open PDF position" }));

    await waitFor(() =>
      expect(client.inverseSynctex).toHaveBeenCalledWith(
        projectId,
        operationId,
        2,
        30,
        40,
      ),
    );
    expect(
      await screen.findByRole("tab", { name: "proof.tex" }),
    ).toHaveAttribute("aria-selected", "true");
    expect(readTextFile).toHaveBeenCalledWith(projectId, "sections/proof.tex");
  });

  it("presents, filters, navigates, and exposes raw build diagnostics", async () => {
    const projectId = "8".repeat(64);
    const readTextFile = vi.fn().mockResolvedValue({
      apiVersion: 1,
      relativePath: "main.tex",
      text: "first\nsecond\nthird",
      fingerprint: "7".repeat(64),
      sizeBytes: 18,
    });
    const client = conflictClient(projectId, readTextFile, () => undefined);
    client.readBuildPdf = vi
      .fn()
      .mockImplementation(() => new Promise(() => undefined));
    client.requestBuild = vi.fn().mockResolvedValue({
      apiVersion: 1,
      projectId,
      operationId: "build-9",
      phase: "succeeded",
      reason: "explicit",
      rootDocument: "main.tex",
      engine: "pdfLatex",
      elapsedMs: 1250n,
      exitCode: 0,
      logTruncated: false,
      rawLogAvailable: true,
      diagnostics: [
        {
          code: "LATEX_ERROR",
          severity: "error",
          phase: "latex",
          message: "Undefined control sequence.",
          source: {
            relativePath: "main.tex",
            startLine: 2,
            endLine: 2,
          },
        },
      ],
      pdfAvailable: true,
      lastSuccessfulOperationId: "build-9",
      message: null,
    });
    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    fireEvent.click(await screen.findByRole("button", { name: "Compile" }));

    await waitFor(() =>
      expect(client.requestBuild).toHaveBeenCalledWith(projectId, "explicit"),
    );
    expect(await screen.findByText("Build succeeded in 1.25s")).toBeVisible();
    await waitFor(() =>
      expect(client.readBuildPdf).toHaveBeenCalledWith(projectId, "build-9"),
    );
    expect(screen.getByText("Last successful PDF is available.")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Problems" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /LATEX_ERROR/ }));
    await waitFor(() =>
      expect(readTextFile).toHaveBeenCalledWith(projectId, "main.tex"),
    );
    expect(await screen.findByRole("tab", { name: "main.tex" })).toBeVisible();
    await waitFor(() =>
      expect(document.querySelector(".cm-diagnostic-error")).not.toBeNull(),
    );

    fireEvent.change(screen.getByLabelText("Diagnostic severity"), {
      target: { value: "warning" },
    });
    expect(screen.queryByRole("button", { name: /LATEX_ERROR/ })).toBeNull();

    client.readBuildLog = vi.fn().mockResolvedValue({
      apiVersion: 1,
      projectId,
      operationId: "build-9",
      text: "raw latex log",
      truncated: false,
    });
    fireEvent.click(screen.getByRole("button", { name: "Open raw LaTeX log" }));
    expect(await screen.findByText("raw latex log")).toBeVisible();

    const content = document.querySelector<HTMLElement>(".cm-content");
    expect(content).not.toBeNull();
    if (content) {
      content.textContent = "changed after build";
      fireEvent.input(content, {
        inputType: "insertText",
        data: "changed after build",
      });
      expect(
        await screen.findByText(
          "Diagnostics are from an older document version.",
        ),
      ).toBeVisible();
    }

    fireEvent.click(screen.getByRole("button", { name: "Clean" }));
    await waitFor(() =>
      expect(client.cleanBuildArtifacts).toHaveBeenCalledWith(projectId),
    );
  });
  it("inserts the effective preferred notation literally as one undoable change", async () => {
    const projectId = "9".repeat(64);
    const client = conflictClient(
      projectId,
      vi.fn().mockResolvedValue({
        apiVersion: 1,
        relativePath: "main.tex",
        text: "base",
        fingerprint: "1".repeat(64),
        sizeBytes: 4,
      }),
      () => undefined,
    );
    client.notationProfile = vi.fn().mockResolvedValue({
      apiVersion: 1,
      profileVersion: 1,
      name: "Project notation",
      projectId,
      concepts: [
        {
          id: "adversary",
          label: "Adversary",
          preferredForm: "\\Adv$0",
          declaredForms: ["\\Adv$0"],
          source: "project",
        },
      ],
    });

    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    fireEvent.click(await screen.findByRole("button", { name: /main\.tex/ }));
    await screen.findByRole("tab", { name: "main.tex" });
    fireEvent.click(screen.getByRole("button", { name: "Notation" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Insert preferred form" }),
    );

    const content = document.querySelector<HTMLElement>(".cm-content");
    await waitFor(() => expect(content).toHaveTextContent("\\Adv$0base"));
    if (!content) return;
    fireEvent.keyDown(content, { key: "z", ctrlKey: true });
    await waitFor(() => expect(content).toHaveTextContent("base"));
  });

  it("inserts a catalog snippet as one undoable editor change", async () => {
    const projectId = "8".repeat(64);
    const client = conflictClient(
      projectId,
      vi.fn().mockResolvedValue({
        apiVersion: 1,
        relativePath: "main.tex",
        text: "base",
        fingerprint: "1".repeat(64),
        sizeBytes: 4,
      }),
      () => undefined,
    );
    client.searchCatalog = vi.fn().mockResolvedValue([
      {
        entry: {
          id: "latex.emph",
          command: "\\emph",
          displayName: "Emphasis",
          summary: "Emphasizes text.",
          concepts: ["emphasis"],
          synonyms: [],
          requirements: [],
          signature: "\\emph{text}",
          snippet: "\\emph{${1:text}}$0",
          examples: [],
          documentationUrl: "https://latexref.xyz/",
          contexts: ["text"],
          provenance: {
            sourceTitle: "LaTeX",
            sourceUrl: "https://www.latex-project.org/help/documentation/",
            sourceVersion: "2026-06-01",
          },
        },
        score: 1_000,
        matchKind: "exact",
        contextMatch: true,
        requirementsSatisfied: true,
      },
    ]);

    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    fireEvent.click(await screen.findByRole("button", { name: /main\.tex/ }));
    await screen.findByRole("tab", { name: "main.tex" });
    fireEvent.click(screen.getByRole("button", { name: "Command Finder" }));
    fireEvent.click(await screen.findByRole("button", { name: "Insert" }));

    const content = document.querySelector<HTMLElement>(".cm-content");
    await waitFor(() => expect(content).toHaveTextContent("\\emph{text}base"));
    if (!content) return;
    fireEvent.keyDown(content, { key: "z", ctrlKey: true });
    await waitFor(() => expect(content).toHaveTextContent("base"));
    expect(content).not.toHaveTextContent("\\emph");
  });
});

function conflictClient(
  projectId: string,
  readTextFile: BackendClient["readTextFile"],
  capture: (listener: (change: ProjectFileChange) => void) => void,
): BackendClient {
  return {
    health: vi.fn(),
    toolchainReadiness: vi.fn(),
    openProject: vi.fn().mockResolvedValue({
      apiVersion: 1,
      projectId,
      name: "paper",
      canonicalRoot: "/paper",
    }),
    listDirectory: vi.fn().mockResolvedValue({
      apiVersion: 1,
      directory: "",
      truncated: false,
      entries: [
        {
          name: "main.tex",
          relativePath: "main.tex",
          kind: "file",
          isSymlink: false,
          accessible: true,
          hidden: false,
          generated: false,
        },
      ],
    }),
    readTextFile,
    writeTextFile: vi.fn().mockResolvedValue({
      apiVersion: 1,
      relativePath: "main.tex",
      fingerprint: "3".repeat(64),
      sizeBytes: 7,
    }),
    detectRootDocuments: vi.fn().mockResolvedValue(rootCandidates(projectId)),
    setRootDocument: vi.fn().mockResolvedValue(rootCandidates(projectId)),
    ...recoveryMocks(),
    ...trustMocks(),
    ...buildMocks(),
    onProjectFileChange: vi
      .fn()
      .mockImplementation((listener: (change: ProjectFileChange) => void) => {
        capture(listener);
        return Promise.resolve(() => undefined);
      }),
  };
}

function pdfBuildState(
  projectId: string,
  operationId: string,
  phase: BuildState["phase"],
  pdfAvailable: boolean,
): BuildState {
  return {
    apiVersion: 1,
    projectId,
    operationId,
    phase,
    reason: "explicit",
    rootDocument: "main.tex",
    engine: "pdfLatex",
    elapsedMs: phase === "running" ? null : 10n,
    exitCode: phase === "succeeded" ? 0 : null,
    logTruncated: false,
    rawLogAvailable: false,
    diagnostics: [],
    pdfAvailable,
    lastSuccessfulOperationId:
      phase === "succeeded" ? operationId : "build-00000000000000000002",
    message: null,
  };
}

function rootCandidates(projectId: string) {
  void projectId;
  return {
    apiVersion: 1,
    candidates: [
      {
        relativePath: "main.tex",
        reasons: ["documentClass" as const],
      },
    ],
    selected: "main.tex",
  };
}

function buildMocks() {
  return {
    searchCatalog: vi.fn().mockResolvedValue([]),
    notationProfile: vi.fn(),
    notationUsage: vi.fn(),
    setGlobalNotationProfile: vi.fn(),
    resetGlobalNotationProfile: vi.fn(),
    setProjectNotationOverrides: vi.fn(),
    importNotationProfile: vi.fn(),
    exportNotationProfile: vi.fn(),
    projectIndex: vi.fn().mockResolvedValue({
      apiVersion: 1,
      schemaVersion: 1,
      projectId: "a".repeat(64),
      generation: 1n,
      completeness: "bestEffort",
      scannerLimits: {
        maxProjectFiles: 10000,
        maxTotalBytes: 268435456n,
        maxFileBytes: 5242880n,
        maxRecordsPerFile: 50000,
        maxBraceDepth: 256,
        maxCommandBytes: 4096,
      },
      files: [],
      issues: [],
    }),
    resolveBuildConfiguration: vi.fn(),
    setProjectEngine: vi.fn(),
    requestBuild: vi.fn().mockResolvedValue({
      apiVersion: 1,
      projectId: "a".repeat(64),
      operationId: "build-1",
      phase: "running" as const,
      reason: "explicit" as const,
      rootDocument: "main.tex",
      engine: "pdfLatex" as const,
      elapsedMs: null,
      exitCode: null,
      logTruncated: false,
      rawLogAvailable: false,
      diagnostics: [],
      pdfAvailable: false,
      lastSuccessfulOperationId: null,
      message: null,
    }),
    cancelBuild: vi.fn().mockResolvedValue(true),
    cleanBuildArtifacts: vi.fn().mockResolvedValue(undefined),
    readBuildLog: vi.fn().mockRejectedValue(new Error("not used")),
    readBuildPdf: vi.fn().mockRejectedValue(new Error("not used")),
    forwardSynctex: vi.fn().mockResolvedValue(null),
    inverseSynctex: vi.fn().mockResolvedValue(null),
    onBuildState: vi.fn().mockResolvedValue(() => undefined),
    onBuildOutput: vi.fn().mockResolvedValue(() => undefined),
  };
}

function trustMocks() {
  return {
    projectTrust: vi.fn(),
    setProjectPermission: vi.fn(),
    revokeProjectTrust: vi.fn(),
  };
}

function recoveryMocks() {
  return {
    storeRecoverySnapshot: vi
      .fn()
      .mockImplementation(
        (
          projectId: string,
          relativePath: string,
          text: string,
          baseFingerprint: string,
          revision: number,
        ) =>
          Promise.resolve({
            apiVersion: 1,
            projectId,
            relativePath,
            text,
            baseFingerprint,
            revision: BigInt(revision),
            updatedAtMs: 1n,
          }),
      ),
    listRecoverySnapshots: vi.fn().mockResolvedValue({
      apiVersion: 1,
      snapshots: [],
      warnings: [],
    }),
    deleteRecoverySnapshot: vi.fn().mockResolvedValue(undefined),
  };
}
