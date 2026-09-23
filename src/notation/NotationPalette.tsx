import { useEffect, useMemo, useRef, useState } from "react";
import type { EffectiveNotationConcept } from "../bindings/EffectiveNotationConcept";
import type { EffectiveNotationProfile } from "../bindings/EffectiveNotationProfile";

interface Props {
  load(this: void): Promise<EffectiveNotationProfile>;
  onClose(this: void): void;
  onInsert(this: void, concept: EffectiveNotationConcept): void;
}

const sourceLabels = {
  default: "CrypTex default",
  global: "Global profile",
  project: "Project override",
} as const;

export function NotationPalette({ load, onClose, onInsert }: Props) {
  const [profile, setProfile] = useState<EffectiveNotationProfile | null>(null);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [message, setMessage] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let current = true;
    void load()
      .then((value) => {
        if (current) setProfile(value);
      })
      .catch((reason: unknown) => {
        if (current)
          setMessage(reason instanceof Error ? reason.message : String(reason));
      });
    input.current?.focus();
    return () => {
      current = false;
    };
  }, [load]);

  const concepts = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (!profile) return [];
    if (!needle) return profile.concepts;
    return profile.concepts.filter((concept) =>
      [
        concept.id,
        concept.label,
        concept.preferredForm,
        ...concept.declaredForms,
      ].some((value) => value.toLocaleLowerCase().includes(needle)),
    );
  }, [profile, query]);
  const active = concepts[Math.min(selected, Math.max(0, concepts.length - 1))];
  const insertActive = () => {
    if (active) onInsert(active);
  };

  return (
    <div className="finder-backdrop" onMouseDown={onClose}>
      <section
        aria-labelledby="notation-palette-title"
        aria-modal="true"
        className="command-finder notation-palette"
        role="dialog"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          } else if (event.key === "ArrowDown") {
            event.preventDefault();
            setSelected((value) =>
              Math.min(value + 1, Math.max(0, concepts.length - 1)),
            );
          } else if (event.key === "ArrowUp") {
            event.preventDefault();
            setSelected((value) => Math.max(0, value - 1));
          } else if (event.key === "Enter") {
            event.preventDefault();
            insertActive();
          }
        }}
      >
        <header>
          <div>
            <h2 id="notation-palette-title">Notation Palette</h2>
            <p>{profile?.name ?? "Loading the effective notation profile…"}</p>
          </div>
          <button
            aria-label="Close Notation Palette"
            onClick={onClose}
            type="button"
          >
            ×
          </button>
        </header>
        <div className="finder-search notation-search">
          <label>
            <span className="visually-hidden">Search notation</span>
            <input
              ref={input}
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setSelected(0);
              }}
              placeholder="Concept or LaTeX form…"
            />
          </label>
        </div>
        {message ? (
          <p className="palette-message" role="alert">
            {message}
          </p>
        ) : null}
        <div className="finder-body">
          <ul
            aria-label="Notation concepts"
            className="finder-results"
            role="listbox"
          >
            {concepts.map((concept, index) => (
              <li key={concept.id}>
                <button
                  aria-selected={selected === index}
                  className={selected === index ? "selected" : ""}
                  onClick={() => setSelected(index)}
                  onDoubleClick={() => onInsert(concept)}
                  role="option"
                  type="button"
                >
                  <code>{concept.preferredForm}</code>
                  <span>{concept.label}</span>
                  <small>{sourceLabels[concept.source]}</small>
                </button>
              </li>
            ))}
          </ul>
          {active ? (
            <section className="finder-details" aria-live="polite">
              <h3>{active.label}</h3>
              <code>{active.preferredForm}</code>
              <p>
                Preferred form from{" "}
                <strong>{sourceLabels[active.source]}</strong>.
              </p>
              <div className="notation-forms">
                <span>Declared exact forms</span>
                {active.declaredForms.map((form) => (
                  <code key={form}>{form}</code>
                ))}
              </div>
              <button type="button" onClick={insertActive}>
                Insert preferred form
              </button>
            </section>
          ) : profile ? (
            <p className="palette-message">No matching notation.</p>
          ) : (
            <p className="palette-message">Loading…</p>
          )}
        </div>
      </section>
    </div>
  );
}
