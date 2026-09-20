import { readFile } from "node:fs/promises";

import { describe, expect, it } from "vitest";

import {
  DEFAULT_ROOTS,
  measureClosure,
  parseTlpdb,
} from "./measure-texlive.mjs";

const fixture = `name collection-example
category Collection
depend engine.ARCH
depend macros
containersize 10

name engine.x86_64-linux
category Package
containersize 20
catalogue-license gpl2
binfiles arch=x86_64-linux size=7

name macros
category Package
depend setting/value
containersize 30
catalogue-license lppl1.3c
doccontainersize 40
runfiles size=5
docfiles size=6
`;

describe("TeX Live payload measurement", () => {
  it("keeps the measured roots synchronized with the release input", async () => {
    const source = await readFile("toolchain/texlive-2026-roots.txt", "utf8");
    const roots = source
      .split(/\r?\n/u)
      .filter((line) => line && !line.startsWith("#"));

    expect(roots).toEqual(DEFAULT_ROOTS);
  });
  it("resolves platform dependencies and sums archive and installed sizes", () => {
    const result = measureClosure(
      parseTlpdb(fixture),
      ["collection-example"],
      "x86_64-linux",
    );

    expect(result).toEqual({
      roots: ["collection-example"],
      platform: "x86_64-linux",
      packageCount: 3,
      archiveBytes: { runtime: 60, documentation: 40, source: 0 },
      installedBytes: {
        runtime: 12 * 4096,
        documentation: 6 * 4096,
        source: 0,
      },
      licenseCounts: { gpl2: 1, "lppl1.3c": 1 },
      packagesWithoutLicense: 1,
      missing: [],
    });
  });

  it("reports unresolved packages", () => {
    const result = measureClosure(
      parseTlpdb(fixture),
      ["not-there"],
      "x86_64-linux",
    );

    expect(result.missing).toEqual(["not-there"]);
    expect(result.licenseCounts).toEqual({});
    expect(result.packagesWithoutLicense).toBe(0);
  });
});
