#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

export const SUPPORTED_LINUX_TARGET = Object.freeze({
  architecture: "x86_64",
  distribution: "Ubuntu",
  release: "24.04",
  bundles: Object.freeze(["appimage", "deb"]),
});

export function validateLinuxPackaging(config, applicationPackage) {
  const errors = [];
  const targets = config?.bundle?.targets;
  if (!config?.bundle?.active) errors.push("Tauri bundling must be active");
  if (!Array.isArray(targets)) {
    errors.push("bundle.targets must be an explicit array");
  } else {
    const actual = [...targets].sort();
    if (
      JSON.stringify(actual) !== JSON.stringify(SUPPORTED_LINUX_TARGET.bundles)
    ) {
      errors.push("bundle.targets must contain exactly appimage and deb");
    }
  }
  if (config?.identifier !== "org.cryptex.app") {
    errors.push("the stable application identifier must be org.cryptex.app");
  }
  if (config?.version !== "0.1.0") {
    errors.push("the Tauri package version must be 0.1.0");
  }
  if (applicationPackage?.version !== "0.1.0-dev") {
    errors.push(
      "the workspace version must remain the explicit 0.1.0-dev prerelease",
    );
  }
  if (applicationPackage?.license !== "GPL-3.0-or-later") {
    errors.push("the application license must be GPL-3.0-or-later");
  }
  if (applicationPackage?.private !== true) {
    errors.push(
      "the desktop workspace must not be published as an npm package",
    );
  }
  if (errors.length > 0) throw new Error(errors.join("\n"));
  return SUPPORTED_LINUX_TARGET;
}

async function main() {
  const [config, applicationPackage] = await Promise.all([
    readFile(
      new URL("../src-tauri/tauri.conf.json", import.meta.url),
      "utf8",
    ).then(JSON.parse),
    readFile(new URL("../package.json", import.meta.url), "utf8").then(
      JSON.parse,
    ),
  ]);
  const target = validateLinuxPackaging(config, applicationPackage);
  process.stdout.write(
    `Linux packaging configuration valid: ${target.distribution} ${target.release} ${target.architecture}; ${target.bundles.join(", ")}\n`,
  );
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  await main();
}
