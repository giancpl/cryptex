import { useEffect, useRef, useState } from "react";
import type { SynctexPosition } from "../bindings/SynctexPosition";

const MIN_ZOOM = 0.5;
const MAX_ZOOM = 3;
const MAX_CANVAS_PIXELS = 16_000_000;
const MAX_SEARCH_PAGES = 500;

interface PdfViewport {
  width: number;
  height: number;
}
interface PdfTextContent {
  items: Array<{ str?: string }>;
}
interface PdfPage {
  getViewport(options: { scale: number }): PdfViewport;
  getTextContent(): Promise<PdfTextContent>;
  render(options: {
    canvas: HTMLCanvasElement;
    viewport: PdfViewport;
    annotationMode: number;
  }): {
    promise: Promise<void>;
    cancel(): void;
  };
}
interface PdfDocument {
  numPages: number;
  getPage(page: number): Promise<PdfPage>;
  destroy(): Promise<void>;
}

export type PdfDocumentLoader = (data: Uint8Array) => Promise<PdfDocument>;

interface PdfViewerProps {
  projectId: string;
  operationId: string;
  data: Uint8Array;
  forwardTarget?: (SynctexPosition & { requestId: number }) | undefined;
  onInverseSearch?: (page: number, x: number, y: number) => void;
  loadDocument?: PdfDocumentLoader;
}

export function PdfViewer({
  projectId,
  operationId,
  data,
  forwardTarget,
  onInverseSearch,
  loadDocument = loadPdfDocument,
}: PdfViewerProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const marker = useRef<HTMLDivElement>(null);
  const [document, setDocument] = useState<PdfDocument | null>(null);
  const [page, setPage] = useState(() => storedView(projectId).page);
  const [zoom, setZoom] = useState(() => storedView(projectId).zoom);
  const [renderScale, setRenderScale] = useState(1);
  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState<number[]>([]);
  const [searching, setSearching] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    let loaded: PdfDocument | null = null;
    setDocument(null);
    setMatches([]);
    setMessage(null);
    void loadDocument(data.slice())
      .then((next) => {
        if (disposed) return next.destroy();
        loaded = next;
        setDocument(next);
        setPage((current) => Math.max(1, Math.min(current, next.numPages)));
      })
      .catch((reason: unknown) => {
        if (!disposed) setMessage(errorMessage(reason));
      });
    return () => {
      disposed = true;
      if (loaded) void loaded.destroy();
    };
  }, [data, loadDocument, operationId]);

  useEffect(() => {
    if (!document || !canvas.current) return;
    let disposed = false;
    let renderTask: { promise: Promise<void>; cancel(): void } | null = null;
    setMessage(null);
    void document
      .getPage(page)
      .then((pdfPage) => {
        if (disposed || !canvas.current) return;
        let viewport = pdfPage.getViewport({ scale: zoom });
        const pixels = viewport.width * viewport.height;
        if (pixels > MAX_CANVAS_PIXELS) {
          const safeScale = zoom * Math.sqrt(MAX_CANVAS_PIXELS / pixels);
          viewport = pdfPage.getViewport({ scale: safeScale });
        }
        const target = canvas.current;
        setRenderScale(
          viewport.width / pdfPage.getViewport({ scale: 1 }).width,
        );
        target.width = Math.max(1, Math.floor(viewport.width));
        target.height = Math.max(1, Math.floor(viewport.height));
        renderTask = pdfPage.render({
          canvas: target,
          viewport,
          annotationMode: 0,
        });
        return renderTask.promise;
      })
      .catch((reason: unknown) => {
        if (!disposed && !isCancelled(reason)) setMessage(errorMessage(reason));
      });
    return () => {
      disposed = true;
      renderTask?.cancel();
    };
  }, [document, page, zoom]);

  useEffect(() => {
    if (
      !document ||
      !forwardTarget ||
      forwardTarget.operationId !== operationId
    )
      return;
    if (forwardTarget.page > document.numPages) {
      setMessage("SyncTeX returned a page outside the loaded PDF.");
      return;
    }
    setPage(forwardTarget.page);
  }, [document, forwardTarget, operationId]);

  useEffect(() => {
    if (forwardTarget?.page !== page) return;
    marker.current?.scrollIntoView?.({
      block: "center",
      inline: "center",
    });
  }, [forwardTarget, page, renderScale]);

  useEffect(() => {
    localStorage.setItem(
      viewStorageKey(projectId),
      JSON.stringify({ page, zoom }),
    );
  }, [page, projectId, zoom]);

  async function search() {
    if (!document || !query.trim()) {
      setMatches([]);
      return;
    }
    setSearching(true);
    setMessage(null);
    try {
      const needle = query.trim().toLocaleLowerCase();
      const found: number[] = [];
      const limit = Math.min(document.numPages, MAX_SEARCH_PAGES);
      for (let candidate = 1; candidate <= limit; candidate += 1) {
        const pdfPage = await document.getPage(candidate);
        const content = await pdfPage.getTextContent();
        const text = content.items
          .map((item) => item.str ?? "")
          .join(" ")
          .toLocaleLowerCase();
        if (text.includes(needle)) found.push(candidate);
      }
      setMatches(found);
      if (found[0]) setPage(found[0]);
      if (document.numPages > MAX_SEARCH_PAGES)
        setMessage(
          "Search limited to the first " + MAX_SEARCH_PAGES + " pages.",
        );
    } catch (reason) {
      setMessage(errorMessage(reason));
    } finally {
      setSearching(false);
    }
  }

  return (
    <section className="pdf-viewer" aria-label="PDF preview">
      <div className="pdf-toolbar">
        <button
          type="button"
          aria-label="Previous PDF page"
          disabled={!document || page <= 1}
          onClick={() => setPage((current) => Math.max(1, current - 1))}
        >
          Previous
        </button>
        <label>
          Page
          <input
            aria-label="PDF page"
            type="number"
            min={1}
            max={document?.numPages ?? 1}
            value={page}
            disabled={!document}
            onChange={(event) =>
              setPage(
                Math.max(
                  1,
                  Math.min(Number(event.target.value), document?.numPages ?? 1),
                ),
              )
            }
          />
          <span> / {document?.numPages ?? "—"}</span>
        </label>
        <button
          type="button"
          aria-label="Next PDF page"
          disabled={!document || page >= document.numPages}
          onClick={() =>
            setPage((current) =>
              Math.min(document?.numPages ?? current, current + 1),
            )
          }
        >
          Next
        </button>
        <button
          type="button"
          aria-label="Zoom out PDF"
          onClick={() =>
            setZoom((current) => Math.max(MIN_ZOOM, current - 0.25))
          }
        >
          −
        </button>
        <output aria-label="PDF zoom">{Math.round(zoom * 100)}%</output>
        <button
          type="button"
          aria-label="Zoom in PDF"
          onClick={() =>
            setZoom((current) => Math.min(MAX_ZOOM, current + 0.25))
          }
        >
          +
        </button>
      </div>
      <form
        className="pdf-search"
        onSubmit={(event) => {
          event.preventDefault();
          void search();
        }}
      >
        <input
          aria-label="Search PDF"
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <button type="submit" disabled={!document || searching}>
          {searching ? "Searching…" : "Search"}
        </button>
        {matches.length ? (
          <span role="status">Found on {matches.length} page(s).</span>
        ) : null}
      </form>
      {message ? <p role="alert">{message}</p> : null}
      {!document && !message ? <p>Loading PDF…</p> : null}
      <div className="pdf-canvas-container">
        <div className="pdf-page-surface">
          <canvas
            ref={canvas}
            aria-label={"PDF page " + page}
            className={onInverseSearch ? "synctex-clickable" : undefined}
            title={
              onInverseSearch
                ? "Click to open this position in the LaTeX source"
                : undefined
            }
            onClick={(event) => {
              if (!onInverseSearch || renderScale <= 0) return;
              const bounds = event.currentTarget.getBoundingClientRect();
              const x = (event.clientX - bounds.left) / renderScale;
              const y = (event.clientY - bounds.top) / renderScale;
              if (Number.isFinite(x) && Number.isFinite(y) && x >= 0 && y >= 0)
                onInverseSearch(page, x, y);
            }}
          />
          {forwardTarget &&
          forwardTarget.operationId === operationId &&
          forwardTarget.page === page ? (
            <div
              key={forwardTarget.requestId}
              ref={marker}
              className="synctex-marker"
              aria-label="SyncTeX source position"
              style={{
                left: forwardTarget.x * renderScale,
                top:
                  Math.max(0, forwardTarget.y - forwardTarget.height) *
                  renderScale,
                width: Math.max(12, forwardTarget.width * renderScale),
                height: Math.max(12, forwardTarget.height * renderScale),
              }}
            />
          ) : null}
        </div>
      </div>
    </section>
  );
}

async function loadPdfDocument(data: Uint8Array): Promise<PdfDocument> {
  const [{ GlobalWorkerOptions, getDocument }, worker] = await Promise.all([
    import("pdfjs-dist"),
    import("pdfjs-dist/build/pdf.worker.min.mjs?url"),
  ]);
  GlobalWorkerOptions.workerSrc = worker.default;
  const task = getDocument({
    data,
    stopAtErrors: true,
    maxImageSize: MAX_CANVAS_PIXELS,
    useWorkerFetch: false,
    useWasm: false,
    enableXfa: false,
  });
  const document = await task.promise;
  return {
    numPages: document.numPages,
    getPage: async (page) => (await document.getPage(page)) as PdfPage,
    destroy: () => task.destroy(),
  };
}

function storedView(projectId: string): { page: number; zoom: number } {
  try {
    const value = JSON.parse(
      localStorage.getItem(viewStorageKey(projectId)) ?? "null",
    ) as { page?: unknown; zoom?: unknown } | null;
    return {
      page:
        typeof value?.page === "number" &&
        Number.isInteger(value.page) &&
        value.page >= 1
          ? value.page
          : 1,
      zoom:
        typeof value?.zoom === "number" &&
        Number.isFinite(value.zoom) &&
        value.zoom >= MIN_ZOOM &&
        value.zoom <= MAX_ZOOM
          ? value.zoom
          : 1,
    };
  } catch {
    return { page: 1, zoom: 1 };
  }
}

function viewStorageKey(projectId: string): string {
  return "cryptex.pdf-view." + projectId;
}

function isCancelled(reason: unknown): boolean {
  return (
    reason instanceof Error && reason.name === "RenderingCancelledException"
  );
}

function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
