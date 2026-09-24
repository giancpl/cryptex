import { describe, expect, it } from "vitest";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import {
  loadManifest,
  validateArtifactBytes,
  validateFixtureSet,
} from "./run-acceptance-fixtures.mjs";

const manifestPath = resolve("tests/acceptance/manifest.json");
const fixturesRoot = resolve("tests/acceptance/fixtures");

describe("acceptance fixture harness", () => {
  it("validates the complete checked-in matrix", async () => {
    const manifest = await loadManifest(manifestPath);
    await expect(validateFixtureSet(manifest, fixturesRoot)).resolves.toEqual({
      fixtures: 32,
    });
  });

  it("detects an altered expected fixture outcome", async () => {
    const manifest = await loadManifest(manifestPath);
    manifest.fixtures
      .find((fixture) => fixture.id === "minimal")
      .requiredFiles.push("missing.expected");
    await expect(validateFixtureSet(manifest, fixturesRoot)).rejects.toThrow(
      "missing.expected",
    );
  });

  it("rejects malformed PDF and SyncTeX artifacts", () => {
    expect(() =>
      validateArtifactBytes(
        "main.pdf",
        Buffer.from("%PDF-1.7\nmissing trailer"),
      ),
    ).toThrow("invalid PDF artifact");
    expect(() =>
      validateArtifactBytes("main.synctex.gz", Buffer.from("not gzip")),
    ).toThrow("invalid compressed SyncTeX artifact");
    expect(() =>
      validateArtifactBytes("main.pdf", Buffer.from("%PDF-1.7\n%%EOF\n")),
    ).not.toThrow();
    expect(() =>
      validateArtifactBytes("main.synctex.gz", Buffer.from([0x1f, 0x8b, 0x08])),
    ).not.toThrow();
  });

  it("rejects incorrectly stemmed build artifacts", async () => {
    const manifest = await loadManifest(manifestPath);
    manifest.fixtures.find(
      (fixture) => fixture.id === "minimal",
    ).expectedArtifacts[0] = "main.tex.pdf";
    await expect(validateFixtureSet(manifest, fixturesRoot)).rejects.toThrow(
      "invalid artifact expectations",
    );
  });

  it("detects a missing matrix entry", async () => {
    const manifest = await loadManifest(manifestPath);
    manifest.fixtures = manifest.fixtures.filter(
      (fixture) => fixture.id !== "cryptocode",
    );
    await expect(validateFixtureSet(manifest, fixturesRoot)).rejects.toThrow(
      "missing required fixture: cryptocode",
    );
  });
});
