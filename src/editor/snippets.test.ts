import { describe, expect, it } from "vitest";
import { expandCatalogSnippet } from "./snippets";

describe("expandCatalogSnippet", () => {
  it("expands placeholders and selects the first field", () => {
    expect(expandCatalogSnippet("\\section{${1:Title}}\n$0", "")).toEqual({
      text: "\\section{Title}\n",
      selectionFrom: 9,
      selectionTo: 14,
    });
  });

  it("uses the editor selection for the first placeholder", () => {
    expect(expandCatalogSnippet("\\emph{${1:text}}$0", "chosen")).toEqual({
      text: "\\emph{chosen}",
      selectionFrom: 6,
      selectionTo: 12,
    });
  });

  it("preserves literal dollars and uses the final cursor without fields", () => {
    expect(expandCatalogSnippet("$x$ $0", "")).toEqual({
      text: "$x$ ",
      selectionFrom: 4,
      selectionTo: 4,
    });
  });
});
