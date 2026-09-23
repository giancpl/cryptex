import { Compartment, EditorState, StateEffect } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { useEffect, useRef } from "react";
import { setDiagnosticMarkers, type DiagnosticMarker } from "./editorState";
import { expandCatalogSnippet } from "./snippets";
import { catalogAssistance, type CatalogLookup } from "./catalogAssistance";

export function CodeEditor({
  state,
  diagnostics = [],
  navigation,
  insertion,
  onInsertionApplied,
  catalogSearch,
}: {
  state: EditorState;
  diagnostics?: readonly DiagnosticMarker[];
  navigation?:
    { line: number; column: number | null; request: number } | undefined;
  insertion?:
    { request: number; snippet: string; literal?: boolean } | undefined;
  onInsertionApplied?: (request: number) => void;
  catalogSearch?: CatalogLookup;
}) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const initialState = useRef(state);
  const assistance = useRef(new Compartment());
  const assistanceConfigured = useRef(false);

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
    const editor = view.current;
    if (!editor) return;
    const extension = catalogSearch ? catalogAssistance(catalogSearch) : [];
    editor.dispatch({
      effects: assistanceConfigured.current
        ? assistance.current.reconfigure(extension)
        : StateEffect.appendConfig.of(assistance.current.of(extension)),
    });
    assistanceConfigured.current = true;
  }, [catalogSearch]);

  useEffect(() => {
    if (!view.current || !navigation) return;
    const line = view.current.state.doc.line(
      Math.max(1, Math.min(navigation.line, view.current.state.doc.lines)),
    );
    const anchor =
      line.from +
      Math.max(0, Math.min((navigation.column ?? 1) - 1, line.length));
    view.current.dispatch({
      selection: { anchor },
      effects: EditorView.scrollIntoView(anchor, { y: "center" }),
    });
    view.current.focus();
  }, [navigation]);

  useEffect(() => {
    view.current?.dispatch({ effects: setDiagnosticMarkers.of(diagnostics) });
  }, [diagnostics]);

  useEffect(() => {
    const editor = view.current;
    if (!editor || !insertion) return;
    const selection = editor.state.selection.main;
    const expanded = insertion.literal
      ? {
          text: insertion.snippet,
          selectionFrom: insertion.snippet.length,
          selectionTo: insertion.snippet.length,
        }
      : expandCatalogSnippet(
          insertion.snippet,
          editor.state.sliceDoc(selection.from, selection.to),
        );
    editor.dispatch({
      changes: {
        from: selection.from,
        to: selection.to,
        insert: expanded.text,
      },
      selection: {
        anchor: selection.from + expanded.selectionFrom,
        head: selection.from + expanded.selectionTo,
      },
      scrollIntoView: true,
    });
    editor.focus();
    onInsertionApplied?.(insertion.request);
  }, [insertion, onInsertionApplied]);

  return <div className="code-editor" ref={host} />;
}
