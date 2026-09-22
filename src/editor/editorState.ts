import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { StreamLanguage, bracketMatching } from "@codemirror/language";
import { stex } from "@codemirror/legacy-modes/mode/stex";
import { EditorState, StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, keymap } from "@codemirror/view";

export interface DiagnosticMarker {
  line: number;
  severity: "error" | "warning" | "information";
}

export const setDiagnosticMarkers =
  StateEffect.define<readonly DiagnosticMarker[]>();

const diagnosticMarkers = StateField.define({
  create: () => Decoration.none,
  update: (markers, transaction) => {
    let next = markers.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (!effect.is(setDiagnosticMarkers)) continue;
      const ranges = [...effect.value]
        .filter(
          (marker) =>
            marker.line >= 1 && marker.line <= transaction.newDoc.lines,
        )
        .sort((left, right) => left.line - right.line)
        .map((marker) =>
          Decoration.line({
            class: `cm-diagnostic-line cm-diagnostic-${marker.severity}`,
          }).range(transaction.newDoc.line(marker.line).from),
        );
      next = Decoration.set(ranges, true);
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

export function createLatexEditorState(
  text: string,
  onUpdate: (state: EditorState, changed: boolean) => void,
  onSave: () => void,
): EditorState {
  return EditorState.create({
    doc: text,
    extensions: [
      history(),
      bracketMatching(),
      StreamLanguage.define(stex),
      keymap.of([
        {
          key: "Mod-s",
          preventDefault: true,
          run: () => {
            onSave();
            return true;
          },
        },
        ...defaultKeymap,
        ...historyKeymap,
      ]),
      EditorView.lineWrapping,
      diagnosticMarkers,
      EditorView.updateListener.of((update) =>
        onUpdate(update.state, update.docChanged),
      ),
      EditorView.theme({
        "&": { height: "100%", backgroundColor: "#101418", color: "#d8dee9" },
        ".cm-content": { caretColor: "#f0b35b", padding: "16px" },
        ".cm-gutters": {
          backgroundColor: "#151b21",
          color: "#65717d",
          border: "none",
        },
        ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "#182027" },
        ".cm-diagnostic-line": { boxShadow: "inset 3px 0 transparent" },
        ".cm-diagnostic-error": {
          backgroundColor: "#35191d",
          boxShadow: "inset 3px 0 #d35b66",
        },
        ".cm-diagnostic-warning": {
          backgroundColor: "#322817",
          boxShadow: "inset 3px 0 #d2a75a",
        },
        ".cm-diagnostic-information": {
          backgroundColor: "#172936",
          boxShadow: "inset 3px 0 #5aa6d2",
        },
      }),
    ],
  });
}
