import { describe, expect, it } from "vitest";

import { validateLinuxPackaging } from "./check-linux-packaging.mjs";

const config = {
  bundle: { active: true, targets: ["deb", "appimage"] },
  identifier: "org.cryptex.app",
  version: "0.1.0",
};
const applicationPackage = {
  license: "GPL-3.0-or-later",
  private: true,
  version: "0.1.0-dev",
};

describe("Linux packaging contract", () => {
  it("accepts the frozen prerelease configuration", () => {
    expect(validateLinuxPackaging(config, applicationPackage)).toMatchObject({
      architecture: "x86_64",
      release: "24.04",
    });
  });

  it("rejects implicit, extra, or missing bundle targets", () => {
    expect(() =>
      validateLinuxPackaging(
        { ...config, bundle: { active: true, targets: "all" } },
        applicationPackage,
      ),
    ).toThrow("explicit array");
    expect(() =>
      validateLinuxPackaging(
        {
          ...config,
          bundle: { active: true, targets: ["deb", "appimage", "rpm"] },
        },
        applicationPackage,
      ),
    ).toThrow("exactly appimage and deb");
  });

  it("requires the approved license and disables npm publishing", () => {
    expect(() =>
      validateLinuxPackaging(config, {
        ...applicationPackage,
        license: "UNLICENSED",
      }),
    ).toThrow("license must be GPL-3.0-or-later");
    expect(() =>
      validateLinuxPackaging(config, { ...applicationPackage, private: false }),
    ).toThrow("must not be published as an npm package");
  });
});
