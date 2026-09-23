import { useEffect, useRef, useState } from "react";
import type { CatalogSearchHit } from "../bindings/CatalogSearchHit";
import type { CommandContext } from "../bindings/CommandContext";

interface Props {
  onClose(this: void): void;
  onInsert(this: void, hit: CatalogSearchHit): void;
  search(
    this: void,
    query: string,
    context: CommandContext | null,
  ): Promise<CatalogSearchHit[]>;
}
const contexts: Array<{ value: CommandContext | ""; label: string }> = [
  { value: "", label: "Any context" },
  { value: "text", label: "Text" },
  { value: "math", label: "Math" },
  { value: "environment", label: "Environment" },
  { value: "preamble", label: "Preamble" },
];

export function CommandFinder({ onClose, onInsert, search }: Props) {
  const [query, setQuery] = useState("");
  const [context, setContext] = useState<CommandContext | null>("text");
  const [hits, setHits] = useState<CatalogSearchHit[]>([]);
  const [selected, setSelected] = useState(0);
  const [message, setMessage] = useState<string | null>(null);
  const request = useRef(0);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    input.current?.focus();
  }, []);
  useEffect(() => {
    const current = ++request.current;
    setMessage(null);
    const timer = window.setTimeout(() => {
      void search(query, context)
        .then((results) => {
          if (request.current !== current) return;
          setHits(results);
          setSelected(0);
        })
        .catch((reason: unknown) => {
          if (request.current !== current) return;
          setHits([]);
          setMessage(reason instanceof Error ? reason.message : String(reason));
        });
    }, 80);
    return () => window.clearTimeout(timer);
  }, [context, query, search]);

  const active = hits[selected];
  const insertActive = () => {
    if (active) onInsert(active);
  };
  return (
    <div className="finder-backdrop" onMouseDown={onClose}>
      <section
        aria-labelledby="command-finder-title"
        aria-modal="true"
        className="command-finder"
        role="dialog"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          } else if (event.key === "ArrowDown") {
            event.preventDefault();
            setSelected((value) =>
              Math.min(value + 1, Math.max(0, hits.length - 1)),
            );
          } else if (event.key === "ArrowUp") {
            event.preventDefault();
            setSelected((value) => Math.max(value - 1, 0));
          } else if (event.key === "Enter") {
            event.preventDefault();
            insertActive();
          }
        }}
      >
        <header>
          <div>
            <h2 id="command-finder-title">Command Finder</h2>
            <p>Search the bundled offline LaTeX catalog.</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close Command Finder"
          >
            ×
          </button>
        </header>
        <div className="finder-search">
          <label>
            <span className="visually-hidden">Search commands</span>
            <input
              ref={input}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Command or concept…"
            />
          </label>
          <label>
            <span className="visually-hidden">Editor context</span>
            <select
              aria-label="Editor context"
              value={context ?? ""}
              onChange={(event) =>
                setContext(
                  (event.target.value || null) as CommandContext | null,
                )
              }
            >
              {contexts.map((item) => (
                <option key={item.label} value={item.value}>
                  {item.label}
                </option>
              ))}
            </select>
          </label>
        </div>
        {message ? <p role="alert">{message}</p> : null}
        <div className="finder-body">
          <ul
            aria-label="Command results"
            className="finder-results"
            role="listbox"
          >
            {hits.map((hit, index) => (
              <li key={hit.entry.id}>
                <button
                  aria-selected={selected === index}
                  className={selected === index ? "selected" : ""}
                  onClick={() => setSelected(index)}
                  onDoubleClick={() => onInsert(hit)}
                  role="option"
                  type="button"
                >
                  <code>{hit.entry.command}</code>
                  <span>{hit.entry.displayName}</span>
                  <small>{hit.matchKind}</small>
                </button>
              </li>
            ))}
          </ul>
          {active ? (
            <section className="finder-details" aria-live="polite">
              <h3>{active.entry.displayName}</h3>
              <code>{active.entry.signature}</code>
              <p>{active.entry.summary}</p>
              {!active.requirementsSatisfied &&
              active.entry.requirements.length ? (
                <p className="finder-warning" role="status">
                  Requires{" "}
                  {active.entry.requirements
                    .map((requirement) =>
                      [requirement.package, requirement.versionRequirement]
                        .filter(Boolean)
                        .join(" "),
                    )
                    .join(", ")}
                  . CrypTex will not edit the preamble automatically.
                </p>
              ) : null}
              <small>
                {active.entry.provenance.sourceTitle} ·{" "}
                {active.entry.provenance.sourceVersion}
              </small>
              <button type="button" onClick={insertActive}>
                Insert
              </button>
            </section>
          ) : (
            <p>No matching commands.</p>
          )}
        </div>
      </section>
    </div>
  );
}
