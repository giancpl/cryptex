import { describe, expect, it } from "vitest";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import {
  loadManifest,
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
