import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { useEffect, useRef } from "react";

export function CodeEditor({ state }: { state: EditorState }) {
  const host = useRef<HTMLDivElement>(null);
  const initialState = useRef(state);

  useEffect(() => {
    if (!host.current) return;
    const view = new EditorView({
      state: initialState.current,
      parent: host.current,
    });
    view.focus();
    return () => view.destroy();
  }, []);

  return <div className="code-editor" ref={host} />;
}
