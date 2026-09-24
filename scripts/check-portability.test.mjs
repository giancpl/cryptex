import { describe, expect, it } from "vitest";
import { resolve } from "node:path";
import { loadManifest } from "./run-acceptance-fixtures.mjs";
import {
  PORTABLE_FIXTURES,
  validatePortableFile,
  validatePortableFixtures,
} from "./check-portability.mjs";

const manifestPath = resolve("tests/acceptance/manifest.json");
const fixturesRoot = resolve("tests/acceptance/fixtures");

describe("standard LaTeX portability", () => {
  it("copies and validates every representative project unchanged", async () => {
    const manifest = await loadManifest(manifestPath);
    const result = await validatePortableFixtures(manifest, fixturesRoot);
    expect(result.map((entry) => entry.id)).toEqual(PORTABLE_FIXTURES);
  });

  it("rejects proprietary metadata and LaTeX commands", () => {
    expect(() =>
      validatePortableFile(".cryptex.json", Buffer.from("{}")),
    ).toThrow("proprietary project file");
    expect(() =>
      validatePortableFile("main.tex", Buffer.from("\\cryptexProtocol{demo}")),
    ).toThrow("proprietary LaTeX command");
  });

  it("detects a missing portability declaration", async () => {
    const manifest = await loadManifest(manifestPath);
    manifest.fixtures.find((fixture) => fixture.id === "minimal").portable =
      false;
    await expect(
      validatePortableFixtures(manifest, fixturesRoot),
    ).rejects.toThrow("declaration does not match");
  });
});
