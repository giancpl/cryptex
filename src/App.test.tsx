import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { BackendClient } from "./api/client";

describe("App", () => {
  it("renders the minimal Project, Editor, and PDF workspace", () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: "Project" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Editor" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "PDF" })).toBeInTheDocument();
  });

  it("opens a selected folder and renders its bounded root listing", async () => {
    const openProject = vi.fn().mockResolvedValue({
      apiVersion: 1,
      projectId: "a".repeat(64),
      name: "paper",
      canonicalRoot: "/paper",
    });
    const listDirectory = vi.fn().mockResolvedValue({
      apiVersion: 1,
      directory: "",
      truncated: false,
      entries: [
        {
          name: "main.tex",
          relativePath: "main.tex",
          kind: "file",
          isSymlink: false,
          accessible: true,
          hidden: false,
          generated: false,
        },
      ],
    });
    const readTextFile = vi.fn().mockResolvedValue({
      apiVersion: 1,
      relativePath: "main.tex",
      text: "\\\\documentclass{article}",
      fingerprint: "f".repeat(64),
      sizeBytes: 23,
    });
    const client: BackendClient = {
      health: vi.fn(),
      openProject,
      listDirectory,
      readTextFile,
      writeTextFile: vi.fn(),
    };
    render(
      <App client={client} pickDirectory={() => Promise.resolve("/paper")} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    await waitFor(() =>
      expect(screen.getByText("main.tex")).toBeInTheDocument(),
    );
    expect(openProject).toHaveBeenCalledWith("/paper");
    expect(listDirectory).toHaveBeenCalledWith("a".repeat(64), "");
    fireEvent.click(screen.getByRole("button", { name: /main\.tex/ }));
    await waitFor(() =>
      expect(readTextFile).toHaveBeenCalledWith("a".repeat(64), "main.tex"),
    );
    expect(screen.getByRole("tab", { name: "main.tex" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
