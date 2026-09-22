import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PdfViewer, type PdfDocumentLoader } from "./PdfViewer";

describe("PdfViewer", () => {
  it("renders pages, navigates, zooms, searches, and persists project view state", async () => {
    const renderPage = vi.fn(() => ({
      promise: Promise.resolve(),
      cancel: vi.fn(),
    }));
    const getPage = vi.fn((page: number) =>
      Promise.resolve({
        getViewport: ({ scale }: { scale: number }) => ({
          width: 100 * scale,
          height: 200 * scale,
        }),
        getTextContent: () =>
          Promise.resolve({
            items: [{ str: page === 2 ? "needle appears here" : "other" }],
          }),
        render: renderPage,
      }),
    );
    const destroy = vi.fn().mockResolvedValue(undefined);
    const loader: PdfDocumentLoader = vi.fn().mockResolvedValue({
      numPages: 3,
      getPage,
      destroy,
    });

    const { unmount } = render(
      <PdfViewer
        projectId="project-a"
        operationId="build-1"
        data={new Uint8Array([1, 2, 3])}
        loadDocument={loader}
      />,
    );

    await waitFor(() => expect(renderPage).toHaveBeenCalled());
    fireEvent.click(screen.getByRole("button", { name: "Next PDF page" }));
    await waitFor(() =>
      expect(screen.getByLabelText("PDF page")).toHaveValue(2),
    );
    fireEvent.click(screen.getByRole("button", { name: "Zoom in PDF" }));
    expect(screen.getByLabelText("PDF zoom")).toHaveTextContent("125%");

    fireEvent.change(screen.getByLabelText("Search PDF"), {
      target: { value: "needle" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    expect(await screen.findByText("Found on 1 page(s).")).toBeVisible();
    expect(screen.getByLabelText("PDF page")).toHaveValue(2);
    expect(localStorage.getItem("cryptex.pdf-view.project-a")).toContain(
      '"zoom":1.25',
    );

    unmount();
    expect(destroy).toHaveBeenCalled();
  });

  it("contains malformed PDF failures without crashing", async () => {
    const loader: PdfDocumentLoader = vi
      .fn()
      .mockRejectedValue(new Error("Malformed PDF"));
    render(
      <PdfViewer
        projectId="project-b"
        operationId="build-2"
        data={new Uint8Array([0])}
        loadDocument={loader}
      />,
    );
    expect(await screen.findByRole("alert")).toHaveTextContent("Malformed PDF");
  });
});
