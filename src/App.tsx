const panes = [
  { title: "Project", detail: "Open a LaTeX project to browse its files." },
  { title: "Editor", detail: "Select a text file to begin editing." },
  { title: "PDF", detail: "A successful build will appear here." },
] as const;

export function App() {
  return (
    <main className="workspace" aria-label="CrypTex workspace">
      <header className="titlebar">
        <span className="wordmark">CrypTex</span>
        <span className="status">Foundation preview</span>
      </header>
      <div className="panes">
        {panes.map((pane) => (
          <section
            className="pane"
            aria-labelledby={`pane-${pane.title}`}
            key={pane.title}
          >
            <h1 id={`pane-${pane.title}`}>{pane.title}</h1>
            <p>{pane.detail}</p>
          </section>
        ))}
      </div>
    </main>
  );
}
