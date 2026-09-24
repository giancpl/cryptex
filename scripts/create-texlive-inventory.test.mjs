import { createHash } from "node:crypto";

import { describe, expect, it } from "vitest";

import { createTexliveInventory } from "./create-texlive-inventory.mjs";

const source = `name collection-example
depend engine.ARCH
depend macros
containersize 10

name engine.x86_64-linux
containersize 20
catalogue-license gpl2
binfiles arch=x86_64-linux size=7

name macros
depend setting/value
containersize 30
catalogue-license lppl1.3c
doccontainersize 40
srccontainersize 5
runfiles size=5
docfiles size=6
srcfiles size=2
`;
const digest = createHash("sha512").update(source).digest("hex");

describe("TeX Live release inventory", () => {
  it("emits a deterministic sorted package and license inventory", () => {
    const inventory = createTexliveInventory({
      source,
      roots: ["collection-example"],
      platform: "x86_64-linux",
      revision: 80315,
      expectedSha512: digest,
    });

    expect(inventory).toMatchObject({
      formatVersion: 1,
      texliveYear: 2026,
      texliveRevision: 80315,
      tlpdbSha512: digest,
      packageCount: 3,
      packagesWithoutDeclaredLicense: ["collection-example"],
    });
    expect(inventory.packages.map(({ name }) => name)).toEqual([
      "collection-example",
      "engine.x86_64-linux",
      "macros",
    ]);
    expect(inventory.packages[2]).toMatchObject({
      dependencies: [],
      license: "lppl1.3c",
      archiveBytes: { runtime: 30, documentation: 40, source: 5 },
      installedBytes: {
        runtime: 5 * 4096,
        documentation: 6 * 4096,
        source: 2 * 4096,
      },
    });
  });

  it("rejects a changed database and unresolved closure", () => {
    const base = {
      source,
      roots: ["collection-example"],
      platform: "x86_64-linux",
      revision: 80315,
      expectedSha512: digest,
    };
    expect(() =>
      createTexliveInventory({ ...base, source: `${source}changed` }),
    ).toThrow("SHA-512 verification failed");
    expect(() =>
      createTexliveInventory({ ...base, roots: ["missing"] }),
    ).toThrow("unresolved TeX Live packages: missing");
  });

  it("rejects malformed release provenance", () => {
    expect(() =>
      createTexliveInventory({
        source,
        roots: ["collection-example"],
        platform: "x86_64-linux",
        revision: 0,
        expectedSha512: digest,
      }),
    ).toThrow("revision or expected");
  });
});
