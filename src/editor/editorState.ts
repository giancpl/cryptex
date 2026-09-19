import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { StreamLanguage, bracketMatching } from "@codemirror/language";
import { stex } from "@codemirror/legacy-modes/mode/stex";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";

export function createLatexEditorState(
  text: string,
  onUpdate: (state: EditorState, changed: boolean) => void,
): EditorState {
  return EditorState.create({
    doc: text,
    extensions: [
      history(),
      bracketMatching(),
      StreamLanguage.define(stex),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      EditorView.lineWrapping,
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
      }),
    ],
  });
}
