import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { EffectiveNotationProfile } from "../bindings/EffectiveNotationProfile";
import { NotationPalette } from "./NotationPalette";

const profile: EffectiveNotationProfile = {
  apiVersion: 1,
  profileVersion: 1,
  name: "Paper notation",
  projectId: "a".repeat(64),
  concepts: [
    {
      id: "adversary",
      label: "Adversary",
      preferredForm: "\\mathsf{Adv}",
      declaredForms: ["\\mathsf{Adv}", "\\mathcal{A}"],
      source: "project",
    },
    {
      id: "probability",
      label: "Probability",
      preferredForm: "\\Pr",
      declaredForms: ["\\Pr"],
      source: "default",
    },
  ],
};

describe("NotationPalette", () => {
  it("searches declared forms, explains precedence, and inserts explicitly", async () => {
    const onInsert = vi.fn();
    render(
      <NotationPalette
        load={() => Promise.resolve(profile)}
        onClose={vi.fn()}
        onInsert={onInsert}
      />,
    );
    expect(await screen.findByText("Paper notation")).toBeVisible();
    fireEvent.change(screen.getByPlaceholderText("Concept or LaTeX form…"), {
      target: { value: "mathcal" },
    });
    expect(screen.getAllByText("Project override")).toHaveLength(2);
    expect(screen.getByText(/Preferred form from/)).toHaveTextContent(
      "Project override",
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Insert preferred form" }),
    );
    expect(onInsert).toHaveBeenCalledWith(profile.concepts[0]);
  });

  it("supports keyboard selection and reports loading failures", async () => {
    const onInsert = vi.fn();
    const { unmount } = render(
      <NotationPalette
        load={() => Promise.resolve(profile)}
        onClose={vi.fn()}
        onInsert={onInsert}
      />,
    );
    const dialog = screen.getByRole("dialog");
    await screen.findByText("Paper notation");
    fireEvent.keyDown(dialog, { key: "ArrowDown" });
    fireEvent.keyDown(dialog, { key: "Enter" });
    expect(onInsert).toHaveBeenCalledWith(profile.concepts[1]);
    unmount();

    render(
      <NotationPalette
        load={() => Promise.reject(new Error("Profile unavailable"))}
        onClose={vi.fn()}
        onInsert={vi.fn()}
      />,
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Profile unavailable",
    );
  });
});
