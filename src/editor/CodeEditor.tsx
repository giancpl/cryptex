import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { useEffect, useRef } from "react";
import { setDiagnosticMarkers, type DiagnosticMarker } from "./editorState";

export function CodeEditor({
  state,
  diagnostics = [],
  navigation,
}: {
  state: EditorState;
  diagnostics?: readonly DiagnosticMarker[];
  navigation?: { line: number; request: number } | undefined;
}) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const initialState = useRef(state);

  useEffect(() => {
    if (!host.current) return;
    const editor = new EditorView({
      state: initialState.current,
      parent: host.current,
    });
    view.current = editor;
    editor.focus();
    return () => {
      view.current = null;
      editor.destroy();
    };
  }, []);

  useEffect(() => {
    if (!view.current || !navigation) return;
    const line = view.current.state.doc.line(
      Math.max(1, Math.min(navigation.line, view.current.state.doc.lines)),
    );
    view.current.dispatch({
      selection: { anchor: line.from },
      effects: EditorView.scrollIntoView(line.from, { y: "center" }),
    });
    view.current.focus();
  }, [navigation]);

  useEffect(() => {
    view.current?.dispatch({ effects: setDiagnosticMarkers.of(diagnostics) });
  }, [diagnostics]);

  return <div className="code-editor" ref={host} />;
}
