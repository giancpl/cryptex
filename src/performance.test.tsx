import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { App } from "./App";
import type { BackendClient } from "./api/client";

it("renders the cold workspace shell within the startup budget", () => {
  const client = new Proxy(
    {},
    {
      get: () => vi.fn().mockRejectedValue(new Error("not used")),
    },
  ) as BackendClient;
  const started = performance.now();
  render(<App client={client} pickDirectory={() => Promise.resolve(null)} />);
  expect(screen.getByRole("main", { name: "CrypTex workspace" })).toBeVisible();
  const elapsed = performance.now() - started;
  expect(elapsed).toBeLessThan(2_000);
});
