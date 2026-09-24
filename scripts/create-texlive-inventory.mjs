#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFile, rename, stat, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import {
  DEFAULT_ROOTS,
  concreteDependency,
  parseTlpdb,
  resolveClosure,
} from "./measure-texlive.mjs";

const MAX_TLPDB_BYTES = 64 * 1024 * 1024;
const RELEASE_PLATFORM = "x86_64-linux";
const RELEASE_YEAR = 2026;

function sha512(source) {
  return createHash("sha512").update(source).digest("hex");
}

export function createTexliveInventory({
  source,
  roots,
  platform,
  revision,
  expectedSha512,
}) {
  if (
    !Number.isSafeInteger(revision) ||
    revision <= 0 ||
    !/^[a-f\d]{128}$/u.test(expectedSha512)
  ) {
    throw new Error("revision or expected TLPDB SHA-512 is invalid");
  }
  const actualSha512 = sha512(source);
  if (actualSha512 !== expectedSha512) {
    throw new Error("TLPDB SHA-512 verification failed");
  }
  const database = parseTlpdb(source);
  const closure = resolveClosure(database, roots, platform);
  if (closure.missing.length > 0) {
    throw new Error(
      `unresolved TeX Live packages: ${closure.missing.join(", ")}`,
    );
  }

  const packages = closure.selected.map((name) => {
    const entry = database.get(name);
    return {
      name,
      license: entry.license ?? null,
      dependencies: entry.dependencies
        .map((dependency) => concreteDependency(dependency, platform))
        .filter(Boolean)
        .sort(),
      archiveBytes: entry.archiveBytes,
      installedBytes: Object.fromEntries(
        Object.entries(entry.installedBlocks).map(([kind, blocks]) => [
          kind,
          blocks * 4096,
        ]),
      ),
    };
  });

  return {
    formatVersion: 1,
    texliveYear: RELEASE_YEAR,
    texliveRevision: revision,
    platform,
    tlpdbSha512: actualSha512,
    roots: [...roots],
    packageCount: packages.length,
    packagesWithoutDeclaredLicense: packages
      .filter((entry) => entry.license === null)
      .map((entry) => entry.name),
    packages,
  };
}

function parseArguments(arguments_) {
  const options = { platform: RELEASE_PLATFORM };
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    const value = arguments_.at(++index);
    if (!value) throw new Error(`missing value for ${argument}`);
    if (argument === "--tlpdb") options.tlpdb = value;
    else if (argument === "--revision") options.revision = Number(value);
    else if (argument === "--expected-sha512") options.expectedSha512 = value;
    else if (argument === "--output") options.output = value;
    else if (argument === "--platform") options.platform = value;
    else throw new Error(`unexpected argument: ${argument}`);
  }
  if (
    !options.tlpdb ||
    !options.output ||
    !options.expectedSha512 ||
    !options.revision
  ) {
    throw new Error(
      "usage: create-texlive-inventory --tlpdb FILE --revision NUMBER " +
        "--expected-sha512 HEX --output FILE",
    );
  }
  if (options.platform !== RELEASE_PLATFORM) {
    throw new Error(`unsupported release platform: ${options.platform}`);
  }
  return options;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const metadata = await stat(options.tlpdb);
  if (!metadata.isFile() || metadata.size > MAX_TLPDB_BYTES) {
    throw new Error("TLPDB must be a regular file no larger than 64 MiB");
  }
  const source = await readFile(options.tlpdb, "utf8");
  const inventory = createTexliveInventory({
    source,
    roots: DEFAULT_ROOTS,
    platform: options.platform,
    revision: options.revision,
    expectedSha512: options.expectedSha512,
  });
  const output = resolve(options.output);
  const temporary = `${output}.tmp-${process.pid}`;
  await writeFile(temporary, `${JSON.stringify(inventory, undefined, 2)}\n`, {
    encoding: "utf8",
    flag: "wx",
    mode: 0o600,
  });
  try {
    await rename(temporary, output);
  } catch (error) {
    throw new Error(
      `unable to publish inventory in ${dirname(output)}: ${error}`,
    );
  }
  process.stdout.write(
    `TeX Live inventory: ${inventory.packageCount} packages; ${inventory.packagesWithoutDeclaredLicense.length} require manual license review\n`,
  );
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  await main();
}
