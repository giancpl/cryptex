import type { HealthResponse } from "../bindings/HealthResponse";
import { expect, it } from "vitest";
import type { BackendClient } from "./client";

it("exposes a mockable typed backend contract", async () => {
  const expected: HealthResponse = {
    apiVersion: 1,
    application: "CrypTex",
    version: "test",
  };
  const client: BackendClient = {
    health: () => Promise.resolve(expected),
    toolchainReadiness: () => Promise.reject(new Error("not used")),
    openProject: () => Promise.reject(new Error("not used")),
    listDirectory: () => Promise.reject(new Error("not used")),
    readTextFile: () => Promise.reject(new Error("not used")),
    writeTextFile: () => Promise.reject(new Error("not used")),
    detectRootDocuments: () => Promise.reject(new Error("not used")),
    setRootDocument: () => Promise.reject(new Error("not used")),
    projectTrust: () => Promise.reject(new Error("not used")),
    setProjectPermission: () => Promise.reject(new Error("not used")),
    revokeProjectTrust: () => Promise.reject(new Error("not used")),
    storeRecoverySnapshot: () => Promise.reject(new Error("not used")),
    listRecoverySnapshots: () => Promise.reject(new Error("not used")),
    deleteRecoverySnapshot: () => Promise.reject(new Error("not used")),
    onProjectFileChange: () => Promise.resolve(() => undefined),
  };

  await expect(client.health()).resolves.toEqual(expected);
});
