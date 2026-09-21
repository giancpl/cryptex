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
import type { ProjectFileChange } from "./bindings/ProjectFileChange";

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

  it("starts an explicit build and exposes its terminal status and cleanup", async () => {
    const projectId = "8".repeat(64);
    const client = conflictClient(projectId, vi.fn(), () => undefined);
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
    expect(screen.getByText("Last successful PDF is available.")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Clean" }));
    await waitFor(() =>
      expect(client.cleanBuildArtifacts).toHaveBeenCalledWith(projectId),
    );
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
      pdfAvailable: false,
      lastSuccessfulOperationId: null,
      message: null,
    }),
    cancelBuild: vi.fn().mockResolvedValue(true),
    cleanBuildArtifacts: vi.fn().mockResolvedValue(undefined),
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
