import { useState } from "react";
import type { ApplyNotationRenameResult } from "../bindings/ApplyNotationRenameResult";
import type { NotationRenameFileSelection } from "../bindings/NotationRenameFileSelection";
import type { NotationRenamePreview } from "../bindings/NotationRenamePreview";

export function NotationRenameReview({
  preview,
  result,
  applying,
  onApply,
  onClose,
}: {
  preview: NotationRenamePreview;
  result: ApplyNotationRenameResult | null;
  applying: boolean;
  onApply: (files: NotationRenameFileSelection[]) => void;
  onClose: () => void;
}) {
  const [selected, setSelected] = useState(
    () => new Set(preview.files.map((file) => file.relativePath)),
  );
  const selections = preview.files
    .filter((file) => selected.has(file.relativePath))
    .map((file) => ({
      relativePath: file.relativePath,
      expectedFingerprint: file.fingerprint,
    }));
  return (
    <div className="rename-backdrop" role="presentation">
      <section
        className="rename-review"
        role="dialog"
        aria-modal="true"
        aria-labelledby="rename-title"
      >
        <header>
          <div>
            <h2 id="rename-title">Review notation replacements</h2>
            <p>
              Replace exact declared forms with{" "}
              <code>{preview.preferredForm}</code>. No file changes until you
              confirm.
            </p>
          </div>
          <button
            type="button"
            aria-label="Close notation replacement review"
            onClick={onClose}
          >
            �
          </button>
        </header>
        {preview.incomplete ? (
          <p role="status">
            Some files changed or could not be scanned and are excluded.
          </p>
        ) : null}
        <div className="rename-files">
          {preview.files.map((file) => (
            <article key={file.relativePath}>
              <label>
                <input
                  type="checkbox"
                  checked={selected.has(file.relativePath)}
                  disabled={applying || result !== null}
                  onChange={() =>
                    setSelected((current) => {
                      const next = new Set(current);
                      if (next.has(file.relativePath))
                        next.delete(file.relativePath);
                      else next.add(file.relativePath);
                      return next;
                    })
                  }
                />
                {file.relativePath} � {file.edits.length} replacement
                {file.edits.length === 1 ? "" : "s"}
              </label>
              <div className="rename-diff">
                <section>
                  <h3>Before</h3>
                  <pre>{file.originalText}</pre>
                </section>
                <section>
                  <h3>After</h3>
                  <pre>{file.revisedText}</pre>
                </section>
              </div>
            </article>
          ))}
          {!preview.files.length ? (
            <p>No exact nonpreferred forms are currently replaceable.</p>
          ) : null}
        </div>
        {result ? (
          <ul className="rename-results" aria-label="Replacement results">
            {result.files.map((file) => (
              <li key={file.relativePath}>
                <strong>{file.status}</strong> {file.relativePath}
                {file.message ? `: ${file.message}` : ""}
              </li>
            ))}
          </ul>
        ) : null}
        <footer>
          <button type="button" onClick={onClose}>
            {result ? "Done" : "Cancel"}
          </button>
          {!result ? (
            <button
              type="button"
              disabled={applying || !selections.length}
              onClick={() => onApply(selections)}
            >
              {applying
                ? "Applying&"
                : `Apply to ${selections.length} file${selections.length === 1 ? "" : "s"}`}
            </button>
          ) : null}
        </footer>
      </section>
    </div>
  );
}
