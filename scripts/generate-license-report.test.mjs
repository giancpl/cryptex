import { describe, expect, it } from "vitest";

import {
  createLicenseReport,
  normalizeCargoMetadata,
  normalizeNpmLicenses,
  renderNotices,
} from "./generate-license-report.mjs";

describe("dependency license report", () => {
  it("normalizes and sorts npm and Cargo metadata without machine paths", () => {
    const npm = normalizeNpmLicenses({
      MIT: [
        {
          name: "zeta",
          versions: ["2.0.0", "1.0.0"],
          paths: ["/machine/path"],
          license: "MIT",
        },
      ],
    });
    const rust = normalizeCargoMetadata({
      packages: [
        {
          name: "workspace",
          version: "0.1.0",
          license: "GPL-3.0-or-later",
          source: null,
        },
        {
          name: "alpha",
          version: "1.0.0",
          license: "Apache-2.0 OR MIT",
          source: "registry+https://example.invalid/index",
        },
      ],
    });
    expect(npm).toEqual([
      { license: "MIT", name: "zeta", version: "1.0.0" },
      { license: "MIT", name: "zeta", version: "2.0.0" },
    ]);
    expect(rust).toEqual([
      {
        license: "Apache-2.0 OR MIT",
        name: "alpha",
        version: "1.0.0",
      },
    ]);
  });

  it("rejects Rust dependencies without declared licenses", () => {
    expect(() =>
      normalizeCargoMetadata({
        packages: [
          {
            name: "unknown",
            version: "1",
            license: null,
            source: "registry+https://example.invalid/index",
          },
        ],
      }),
    ).toThrow("has no SPDX license expression");
  });

  it("renders stable markdown with escaped license expressions", () => {
    const report = createLicenseReport(
      [{ license: "MIT", name: "npm-package", version: "1" }],
      [{ license: "MIT OR Apache-2.0", name: "crate", version: "2" }],
    );
    const notices = renderNotices(report);
    expect(notices).toContain("| npm-package | 1 | MIT |");
    expect(notices).toContain("| crate | 2 | MIT OR Apache-2.0 |");
    expect(notices).toContain("managed TeX Live payload");
  });
});
