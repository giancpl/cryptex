import { invoke } from "@tauri-apps/api/core";
import type { HealthResponse } from "../bindings/HealthResponse";

export interface BackendClient {
  health(): Promise<HealthResponse>;
}

export const backendClient: BackendClient = {
  health: () => invoke<HealthResponse>("health"),
};
