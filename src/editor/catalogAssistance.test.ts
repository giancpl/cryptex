import { CompletionContext } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import { describe, expect, it, vi } from "vitest";
import {
  catalogCompletionSource,
  commandAt,
  latexPositionAt,
  signatureCommandAt,
} from "./catalogAssistance";

describe("catalog assistance context", () => {
  it("detects preamble, text, and math contexts", () => {
    expect(latexPositionAt("\\usepackage{cryptocode}", 5)).toEqual({
      context: "preamble",
      excluded: false,
    });
    const text = "\\begin{document}\nText \\ref{key}";
    expect(latexPositionAt(text, text.length)).toEqual({
      context: "text",
      excluded: false,
    });
    const math = "\\begin{document}\n$x \\sam";
    expect(latexPositionAt(math, math.length)).toEqual({
      context: "math",
      excluded: false,
    });
  });

  it("conservatively excludes comments, verb, and verbatim environments", () => {
    for (const source of [
      "\\begin{document}\n% \\sam",
      "\\begin{document}\n\\verb|\\sam",
      "\\begin{document}\n\\verb*|\\sam",
      "\\begin{document}\n\\begin{verbatim}\n\\sam",
    ]) {
      expect(latexPositionAt(source, source.length).excluded).toBe(true);
    }
    const longDocument =
      "\\begin{document}\n" + "text ".repeat(5_000) + "\\ref";
    expect(latexPositionAt(longDocument, longDocument.length).context).toBe(
      "text",
    );
    const escaped = "\\begin{document}\nText \\% \\ref";
    expect(latexPositionAt(escaped, escaped.length).excluded).toBe(false);
  });

  it("finds command hover ranges and signatures", () => {
    const source = "Text \\pseudocode{body}";
    expect(commandAt(source, source.indexOf("code"))).toEqual({
      from: 5,
      to: 16,
      command: "\\pseudocode",
    });
    expect(signatureCommandAt(source, source.indexOf("body") + 4)).toBe(
      "\\pseudocode",
    );
    expect(signatureCommandAt("% \\ref{key}", 10)).toBe("\\ref");
  });

  it("returns catalog completions and suppresses them in comments", async () => {
    const hit = {
      entry: {
        id: "latex.ref",
        command: "\\ref",
        displayName: "Reference",
        summary: "Prints a reference.",
        concepts: ["reference"],
        synonyms: [],
        requirements: [],
        signature: "\\ref{key}",
        snippet: "\\ref{key}",
        examples: [],
        documentationUrl: "https://latexref.xyz/",
        contexts: ["text" as const],
        provenance: {
          sourceTitle: "LaTeX",
          sourceUrl: "https://latex-project.org",
          sourceVersion: "2026",
        },
      },
      score: 1000,
      matchKind: "prefix" as const,
      contextMatch: true,
      requirementsSatisfied: true,
    };
    const search = vi.fn().mockResolvedValue([hit]);
    const completion = catalogCompletionSource(search);
    const text = "\\begin{document}\n\\re";
    const result = await completion(
      new CompletionContext(
        EditorState.create({ doc: text }),
        text.length,
        false,
      ),
    );
    expect(search).toHaveBeenCalledWith("\\re", "text");
    expect(result?.options[0]?.label).toBe("\\ref");
    expect(result?.options[0]?.detail).toBe("\\ref{key}");

    search.mockClear();
    const comment = "% \\re";
    expect(
      await completion(
        new CompletionContext(
          EditorState.create({ doc: comment }),
          comment.length,
          false,
        ),
      ),
    ).toBeNull();
    expect(search).not.toHaveBeenCalled();
  });
});
