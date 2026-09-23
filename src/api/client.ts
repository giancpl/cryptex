import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { BuildConfiguration } from "../bindings/BuildConfiguration";
import type { BuildPermission } from "../bindings/BuildPermission";
import type { BuildLog } from "../bindings/BuildLog";
import type { BuildOutput } from "../bindings/BuildOutput";
import type { BuildReason } from "../bindings/BuildReason";
import type { BuildState } from "../bindings/BuildState";
import type { CatalogSearchHit } from "../bindings/CatalogSearchHit";
import type { CommandContext } from "../bindings/CommandContext";
import type { FileTreePage } from "../bindings/FileTreePage";
import type { ForwardSynctexRequest } from "../bindings/ForwardSynctexRequest";
import type { HealthResponse } from "../bindings/HealthResponse";
import type { InverseSynctexRequest } from "../bindings/InverseSynctexRequest";
import type { LatexEngine } from "../bindings/LatexEngine";
import type { RecoveryInventory } from "../bindings/RecoveryInventory";
import type { RecoverySnapshot } from "../bindings/RecoverySnapshot";
import type { RootDocumentCandidates } from "../bindings/RootDocumentCandidates";
import type { ProjectSummary } from "../bindings/ProjectSummary";
import type { ProjectIndex } from "../bindings/ProjectIndex";
import type { ProjectTrustState } from "../bindings/ProjectTrustState";
import type { ProjectFileChange } from "../bindings/ProjectFileChange";
import type { TextDocument } from "../bindings/TextDocument";
import type { SynctexPosition } from "../bindings/SynctexPosition";
import type { SynctexSourcePosition } from "../bindings/SynctexSourcePosition";
import type { ToolchainReadiness } from "../bindings/ToolchainReadiness";
import type { WriteResult } from "../bindings/WriteResult";

export interface BackendClient {
  health(): Promise<HealthResponse>;
  toolchainReadiness(): Promise<ToolchainReadiness>;
  openProject(root: string): Promise<ProjectSummary>;
  listDirectory(projectId: string, relativePath: string): Promise<FileTreePage>;
  projectIndex(projectId: string): Promise<ProjectIndex>;
  searchCatalog(
    projectId: string,
    query: string,
    context: CommandContext | null,
    limit: number,
  ): Promise<CatalogSearchHit[]>;
  readTextFile(projectId: string, relativePath: string): Promise<TextDocument>;
  writeTextFile(
    this: void,
    projectId: string,
    relativePath: string,
    text: string,
    expectedFingerprint: string,
  ): Promise<WriteResult>;
  detectRootDocuments(
    this: void,
    projectId: string,
  ): Promise<RootDocumentCandidates>;
  setRootDocument(
    this: void,
    projectId: string,
    relativePath: string,
  ): Promise<RootDocumentCandidates>;
  resolveBuildConfiguration(
    this: void,
    projectId: string,
  ): Promise<BuildConfiguration>;
  setProjectEngine(
    this: void,
    projectId: string,
    engine: LatexEngine | null,
  ): Promise<void>;
  requestBuild(
    this: void,
    projectId: string,
    reason: BuildReason,
  ): Promise<BuildState>;
  cancelBuild(
    this: void,
    projectId: string,
    operationId: string,
  ): Promise<boolean>;
  cleanBuildArtifacts(this: void, projectId: string): Promise<void>;
  readBuildLog(
    this: void,
    projectId: string,
    operationId: string,
  ): Promise<BuildLog>;
  readBuildPdf(
    this: void,
    projectId: string,
    operationId: string,
  ): Promise<Uint8Array>;
  forwardSynctex(
    this: void,
    projectId: string,
    operationId: string,
    relativePath: string,
    line: number,
    column: number,
  ): Promise<SynctexPosition | null>;
  inverseSynctex(
    this: void,
    projectId: string,
    operationId: string,
    page: number,
    x: number,
    y: number,
  ): Promise<SynctexSourcePosition | null>;
  projectTrust(this: void, projectId: string): Promise<ProjectTrustState>;
  setProjectPermission(
    this: void,
    projectId: string,
    permission: BuildPermission,
    allowed: boolean,
  ): Promise<ProjectTrustState>;
  revokeProjectTrust(this: void, projectId: string): Promise<ProjectTrustState>;
  storeRecoverySnapshot(
    this: void,
    projectId: string,
    relativePath: string,
    text: string,
    baseFingerprint: string,
    revision: number,
  ): Promise<RecoverySnapshot>;
  listRecoverySnapshots(
    this: void,
    projectId: string,
  ): Promise<RecoveryInventory>;
  deleteRecoverySnapshot(
    this: void,
    projectId: string,
    relativePath: string,
  ): Promise<void>;
  onProjectFileChange(
    listener: (change: ProjectFileChange) => void,
  ): Promise<() => void>;
  onBuildState(listener: (state: BuildState) => void): Promise<() => void>;
  onBuildOutput(listener: (output: BuildOutput) => void): Promise<() => void>;
}

export const backendClient: BackendClient = {
  health: () => invoke<HealthResponse>("health"),
  toolchainReadiness: () => invoke<ToolchainReadiness>("toolchain_readiness"),
  openProject: (root) => invoke<ProjectSummary>("open_project", { root }),
  listDirectory: (projectId, relativePath) =>
    invoke<FileTreePage>("list_directory", { projectId, relativePath }),
  projectIndex: (projectId) =>
    invoke<ProjectIndex>("project_index", { projectId }),
  searchCatalog: (projectId, query, context, limit) =>
    invoke<CatalogSearchHit[]>("search_command_catalog", {
      projectId,
      query,
      context,
      limit,
    }),
  readTextFile: (projectId, relativePath) =>
    invoke<TextDocument>("read_text_file", { projectId, relativePath }),
  writeTextFile: (projectId, relativePath, text, expectedFingerprint) =>
    invoke<WriteResult>("write_text_file", {
      projectId,
      relativePath,
      text,
      expectedFingerprint,
    }),
  detectRootDocuments: (projectId) =>
    invoke<RootDocumentCandidates>("detect_root_documents", { projectId }),
  setRootDocument: (projectId, relativePath) =>
    invoke<RootDocumentCandidates>("set_root_document", {
      projectId,
      relativePath,
    }),
  resolveBuildConfiguration: (projectId) =>
    invoke<BuildConfiguration>("resolve_build_configuration", { projectId }),
  setProjectEngine: (projectId, engine) =>
    invoke<void>("set_project_engine", { projectId, engine }),
  requestBuild: (projectId, reason) =>
    invoke<BuildState>("request_build", { projectId, reason }),
  cancelBuild: (projectId, operationId) =>
    invoke<boolean>("cancel_build", { projectId, operationId }),
  cleanBuildArtifacts: (projectId) =>
    invoke<void>("clean_build_artifacts", { projectId }),
  readBuildLog: (projectId, operationId) =>
    invoke<BuildLog>("read_build_log", { projectId, operationId }),
  readBuildPdf: async (projectId, operationId) => {
    const response = await invoke<ArrayBuffer>("read_build_pdf", {
      projectId,
      operationId,
    });
    return new Uint8Array(response);
  },
  forwardSynctex: (projectId, operationId, relativePath, line, column) => {
    const request: ForwardSynctexRequest = {
      projectId,
      operationId,
      relativePath,
      line,
      column,
    };
    return invoke<SynctexPosition | null>("forward_synctex", { request });
  },
  inverseSynctex: (projectId, operationId, page, x, y) => {
    const request: InverseSynctexRequest = {
      projectId,
      operationId,
      page,
      x,
      y,
    };
    return invoke<SynctexSourcePosition | null>("inverse_synctex", { request });
  },
  projectTrust: (projectId) =>
    invoke<ProjectTrustState>("project_trust", { projectId }),
  setProjectPermission: (projectId, permission, allowed) =>
    invoke<ProjectTrustState>("set_project_permission", {
      projectId,
      permission,
      allowed,
    }),
  revokeProjectTrust: (projectId) =>
    invoke<ProjectTrustState>("revoke_project_trust", { projectId }),
  storeRecoverySnapshot: (
    projectId,
    relativePath,
    text,
    baseFingerprint,
    revision,
  ) =>
    invoke<RecoverySnapshot>("store_recovery_snapshot", {
      projectId,
      relativePath,
      text,
      baseFingerprint,
      revision,
    }),
  listRecoverySnapshots: (projectId) =>
    invoke<RecoveryInventory>("list_recovery_snapshots", { projectId }),
  deleteRecoverySnapshot: (projectId, relativePath) =>
    invoke<void>("delete_recovery_snapshot", { projectId, relativePath }),
  onProjectFileChange: (listener) =>
    listen<ProjectFileChange>("project-file-change", (event) =>
      listener(event.payload),
    ),
  onBuildState: (listener) =>
    listen<BuildState>("build-state", (event) => listener(event.payload)),
  onBuildOutput: (listener) =>
    listen<BuildOutput>("build-output", (event) => listener(event.payload)),
};

export async function pickProjectDirectory(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}
