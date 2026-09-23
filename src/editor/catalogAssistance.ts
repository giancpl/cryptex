import {
  autocompletion,
  snippetCompletion,
  type CompletionContext,
  type CompletionResult,
  type CompletionSource,
} from "@codemirror/autocomplete";
import type { Extension } from "@codemirror/state";
import {
  EditorView,
  ViewPlugin,
  hoverTooltip,
  type Tooltip,
  type ViewUpdate,
} from "@codemirror/view";
import type { CatalogSearchHit } from "../bindings/CatalogSearchHit";
import type { CommandContext } from "../bindings/CommandContext";

export type CatalogLookup = (
  query: string,
  context: CommandContext | null,
) => Promise<CatalogSearchHit[]>;

interface LatexPosition {
  context: CommandContext;
  excluded: boolean;
}

const verbatimEnvironments = ["verbatim", "verbatim*", "lstlisting", "minted"];

export function latexPositionAt(
  source: string,
  position: number,
): LatexPosition {
  const bounded = source.slice(Math.max(0, position - 20_000), position);
  const line = bounded.slice(bounded.lastIndexOf("\n") + 1);
  const comment = unescapedIndex(line, "%");
  if (comment >= 0) return { context: "text", excluded: true };

  const verb = line.lastIndexOf("\\verb");
  if (verb >= 0) {
    const delimiterOffset = line[verb + 5] === "*" ? 6 : 5;
    const delimiter = line[verb + delimiterOffset];
    if (delimiter && !/\s/.test(delimiter)) {
      const rest = line.slice(verb + delimiterOffset + 1);
      if (!rest.includes(delimiter)) return { context: "text", excluded: true };
    }
  }

  for (const environment of verbatimEnvironments) {
    const begin = bounded.lastIndexOf(`\\begin{${environment}}`);
    const end = bounded.lastIndexOf(`\\end{${environment}}`);
    if (begin > end) return { context: "environment", excluded: true };
  }

  const document = source
    .slice(0, Math.min(position, 64 * 1024))
    .indexOf("\\begin{document}");
  if (document < 0) return { context: "preamble", excluded: false };

  const mathDelimiters = [...line.matchAll(/(?<!\\)\$/g)].length;
  const displayOpen = bounded.lastIndexOf("\\[") > bounded.lastIndexOf("\\]");
  const inlineOpen = bounded.lastIndexOf("\\(") > bounded.lastIndexOf("\\)");
  if (mathDelimiters % 2 === 1 || displayOpen || inlineOpen) {
    return { context: "math", excluded: false };
  }
  return { context: "text", excluded: false };
}

export function commandAt(
  source: string,
  position: number,
): { from: number; to: number; command: string } | null {
  const allowed = (character: string) => /[A-Za-z@]/.test(character);
  let from = position;
  let to = position;
  while (from > 0 && allowed(source[from - 1]!)) from -= 1;
  while (to < source.length && allowed(source[to]!)) to += 1;
  if (from === 0 || source[from - 1] !== "\\" || from === to) return null;
  return { from: from - 1, to, command: source.slice(from - 1, to) };
}

export function signatureCommandAt(
  source: string,
  position: number,
): string | null {
  const line = source.slice(
    source.lastIndexOf("\n", position - 1) + 1,
    position,
  );
  const match = /\\([A-Za-z@]+)(?:\[[^\]\n]*\])?\{[^{}]*$/.exec(line);
  return match ? `\\${match[1]}` : null;
}

export function catalogCompletionSource(
  search: CatalogLookup,
): CompletionSource {
  return async (
    context: CompletionContext,
  ): Promise<CompletionResult | null> => {
    const source = context.state.doc.toString();
    const position = latexPositionAt(source, context.pos);
    if (position.excluded) return null;
    const token = context.matchBefore(/\\[A-Za-z@]*$/);
    if (!token || (!context.explicit && token.text.length < 2)) return null;
    const hits = await search(token.text, position.context).catch(() => []);
    if (context.aborted) return null;
    return {
      from: token.from,
      options: hits.slice(0, 30).map((hit) =>
        snippetCompletion(hit.entry.snippet, {
          label: hit.entry.command,
          displayLabel: hit.entry.command,
          detail: hit.entry.signature,
          type: "keyword",
          boost: Math.min(99, Math.floor(hit.score / 20)),
          info: () => assistanceInfo(hit),
        }),
      ),
      validFor: /\\[A-Za-z@]*$/,
    };
  };
}

export function catalogAssistance(search: CatalogLookup): Extension {
  return [
    autocompletion({
      override: [catalogCompletionSource(search)],
      activateOnTyping: true,
      maxRenderedOptions: 30,
    }),
    hoverTooltip(async (view, position) => {
      const source = view.state.doc.toString();
      const latex = latexPositionAt(source, position);
      if (latex.excluded) return null;
      const token = commandAt(source, position);
      if (!token) return null;
      const hits = await search(token.command, latex.context).catch(() => []);
      const hit = hits.find(
        (candidate) => candidate.entry.command === token.command,
      );
      return hit ? commandTooltip(token.from, token.to, hit) : null;
    }),
    signatureHelp(search),
  ];
}

function assistanceInfo(hit: CatalogSearchHit): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-catalog-info";
  appendLine(dom, hit.entry.signature, "code");
  appendLine(dom, hit.entry.summary, "span");
  appendLine(dom, requirementText(hit), "small");
  appendLine(
    dom,
    `${hit.entry.provenance.sourceTitle} · ${hit.entry.provenance.sourceVersion}`,
    "small",
  );
  return dom;
}

function commandTooltip(
  from: number,
  to: number,
  hit: CatalogSearchHit,
): Tooltip {
  return {
    pos: from,
    end: to,
    above: true,
    create: () => ({ dom: assistanceInfo(hit) }),
  };
}

function signatureHelp(search: CatalogLookup): Extension {
  return ViewPlugin.fromClass(
    class {
      readonly dom = document.createElement("div");
      private timer: number | null = null;
      private generation = 0;
      private destroyed = false;

      constructor(private readonly view: EditorView) {
        this.dom.className = "cm-signature-help";
        this.dom.hidden = true;
        view.dom.appendChild(this.dom);
        this.schedule();
      }

      update(update: ViewUpdate) {
        if (update.docChanged || update.selectionSet) this.schedule();
      }

      destroy() {
        this.destroyed = true;
        this.generation += 1;
        if (this.timer !== null) window.clearTimeout(this.timer);
        this.dom.remove();
      }

      private schedule() {
        if (this.timer !== null) window.clearTimeout(this.timer);
        const generation = ++this.generation;
        const source = this.view.state.doc.toString();
        const position = this.view.state.selection.main.head;
        const latex = latexPositionAt(source, position);
        const command = latex.excluded
          ? null
          : signatureCommandAt(source, position);
        if (!command) {
          this.dom.hidden = true;
          return;
        }
        this.timer = window.setTimeout(() => {
          void search(command, latex.context)
            .then((hits) => {
              if (this.destroyed || generation !== this.generation) return;
              const hit = hits.find(
                (candidate) => candidate.entry.command === command,
              );
              if (!hit) {
                this.dom.hidden = true;
                return;
              }
              this.dom.replaceChildren();
              appendLine(this.dom, hit.entry.signature, "code");
              appendLine(this.dom, requirementText(hit), "small");
              this.dom.hidden = false;
            })
            .catch(() => {
              if (!this.destroyed && generation === this.generation)
                this.dom.hidden = true;
            });
        }, 100);
      }
    },
  );
}

function requirementText(hit: CatalogSearchHit): string {
  if (hit.requirementsSatisfied || hit.entry.requirements.length === 0) {
    return "Requirements available";
  }
  return `Requires ${hit.entry.requirements
    .map((requirement) =>
      [requirement.package, requirement.versionRequirement]
        .filter(Boolean)
        .join(" "),
    )
    .join(", ")}`;
}

function appendLine(
  parent: HTMLElement,
  text: string,
  tag: "code" | "small" | "span",
) {
  const element = document.createElement(tag);
  element.textContent = text;
  parent.appendChild(element);
}

function unescapedIndex(value: string, needle: string): number {
  for (let index = 0; index < value.length; index += 1) {
    if (value[index] !== needle) continue;
    let slashes = 0;
    for (
      let cursor = index - 1;
      cursor >= 0 && value[cursor] === "\\";
      cursor -= 1
    )
      slashes += 1;
    if (slashes % 2 === 0) return index;
  }
  return -1;
}
