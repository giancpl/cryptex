import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CatalogSearchHit } from "../bindings/CatalogSearchHit";
import { CommandFinder } from "./CommandFinder";

const hit: CatalogSearchHit = {
  entry: {
    id: "cryptocode.sample",
    command: "\\sample",
    displayName: "Random sampling",
    summary: "Produces the random-sampling assignment symbol.",
    concepts: ["random sampling"],
    synonyms: ["draw"],
    requirements: [{ package: "cryptocode", versionRequirement: "=0.44" }],
    signature: "x \\sample S",
    snippet: "${1:x} \\sample ${2:S}$0",
    examples: [],
    documentationUrl: "https://ctan.org/pkg/cryptocode",
    contexts: ["math", "environment"],
    provenance: {
      sourceTitle: "Cryptocode",
      sourceUrl: "https://ctan.org/pkg/cryptocode",
      sourceVersion: "0.44",
    },
  },
  score: 1000,
  matchKind: "exact",
  contextMatch: true,
  requirementsSatisfied: false,
};

describe("CommandFinder", () => {
  it("searches by keyboard, warns about packages, and inserts explicitly", async () => {
    const search = vi.fn().mockResolvedValue([hit]);
    const onInsert = vi.fn();
    render(
      <CommandFinder onClose={vi.fn()} onInsert={onInsert} search={search} />,
    );
    const input = screen.getByPlaceholderText("Command or concept…");
    fireEvent.change(input, { target: { value: "draw" } });
    await waitFor(() =>
      expect(search).toHaveBeenLastCalledWith("draw", "text"),
    );
    expect(await screen.findByText(/Requires cryptocode =0.44/)).toBeVisible();
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onInsert).toHaveBeenCalledWith(hit);
  });
  it("closes with Escape", () => {
    const onClose = vi.fn();
    render(
      <CommandFinder
        onClose={onClose}
        onInsert={vi.fn()}
        search={() => Promise.resolve([])}
      />,
    );
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
