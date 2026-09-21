import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { BuildPermission } from "../bindings/BuildPermission";
import type { FileTreePage } from "../bindings/FileTreePage";
import type { HealthResponse } from "../bindings/HealthResponse";
import type { RecoveryInventory } from "../bindings/RecoveryInventory";
import type { RecoverySnapshot } from "../bindings/RecoverySnapshot";
import type { RootDocumentCandidates } from "../bindings/RootDocumentCandidates";
import type { ProjectSummary } from "../bindings/ProjectSummary";
import type { ProjectTrustState } from "../bindings/ProjectTrustState";
import type { ProjectFileChange } from "../bindings/ProjectFileChange";
import type { TextDocument } from "../bindings/TextDocument";
import type { ToolchainReadiness } from "../bindings/ToolchainReadiness";
import type { WriteResult } from "../bindings/WriteResult";

export interface BackendClient {
  health(): Promise<HealthResponse>;
  toolchainReadiness(): Promise<ToolchainReadiness>;
  openProject(root: string): Promise<ProjectSummary>;
  listDirectory(projectId: string, relativePath: string): Promise<FileTreePage>;
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
}

export const backendClient: BackendClient = {
  health: () => invoke<HealthResponse>("health"),
  toolchainReadiness: () => invoke<ToolchainReadiness>("toolchain_readiness"),
  openProject: (root) => invoke<ProjectSummary>("open_project", { root }),
  listDirectory: (projectId, relativePath) =>
    invoke<FileTreePage>("list_directory", { projectId, relativePath }),
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
};

export async function pickProjectDirectory(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}
