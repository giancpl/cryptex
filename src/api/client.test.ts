import type { HealthResponse } from "../bindings/HealthResponse";
import { expect, it } from "vitest";
import type { BackendClient } from "./client";

it("exposes a mockable typed backend contract", async () => {
  const expected: HealthResponse = {
    apiVersion: 1,
    application: "CrypTex",
    version: "test",
  };
  const client: BackendClient = { health: () => Promise.resolve(expected) };

  await expect(client.health()).resolves.toEqual(expected);
});
