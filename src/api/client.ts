import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { FileTreePage } from "../bindings/FileTreePage";
import type { HealthResponse } from "../bindings/HealthResponse";
import type { ProjectSummary } from "../bindings/ProjectSummary";

export interface BackendClient {
  health(): Promise<HealthResponse>;
  openProject(root: string): Promise<ProjectSummary>;
  listDirectory(projectId: string, relativePath: string): Promise<FileTreePage>;
}

export const backendClient: BackendClient = {
  health: () => invoke<HealthResponse>("health"),
  openProject: (root) => invoke<ProjectSummary>("open_project", { root }),
  listDirectory: (projectId, relativePath) =>
    invoke<FileTreePage>("list_directory", { projectId, relativePath }),
};

export async function pickProjectDirectory(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}
