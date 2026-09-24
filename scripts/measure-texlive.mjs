#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

export const DEFAULT_ROOTS = [
  "collection-basic",
  "collection-latex",
  "collection-latexrecommended",
  "collection-latexextra",
  "collection-fontsrecommended",
  "collection-bibtexextra",
  "collection-pictures",
  "collection-xetex",
  "collection-luatex",
  "latexmk",
  "cryptocode",
  "synctex",
];

export function parseTlpdb(source) {
  const packages = new Map();
  let current;

  for (const line of source.split(/\r?\n/u)) {
    if (line.startsWith("name ")) {
      current = {
        name: line.slice(5),
        dependencies: [],
        license: undefined,
        archiveBytes: { runtime: 0, documentation: 0, source: 0 },
        installedBlocks: { runtime: 0, documentation: 0, source: 0 },
      };
      packages.set(current.name, current);
      continue;
    }

    if (!current) continue;
    if (line.startsWith("depend ")) {
      current.dependencies.push(line.slice(7));
      continue;
    }
    if (line.startsWith("catalogue-license ")) {
      current.license = line.slice(18);
      continue;
    }

    const archive = line.match(
      /^(container|doccontainer|srccontainer)size (\d+)$/u,
    );
    if (archive) {
      const kind =
        archive[1] === "container"
          ? "runtime"
          : archive[1] === "doccontainer"
            ? "documentation"
            : "source";
      current.archiveBytes[kind] += Number(archive[2]);
    }

    const installed = line.match(
      /^(runfiles|docfiles|srcfiles|binfiles)(?: arch=\S+)? size=(\d+)$/u,
    );
    if (installed) {
      const kind =
        installed[1] === "docfiles"
          ? "documentation"
          : installed[1] === "srcfiles"
            ? "source"
            : "runtime";
      current.installedBlocks[kind] += Number(installed[2]);
    }
  }

  return packages;
}

export function concreteDependency(dependency, platform) {
  if (dependency.includes("/")) return undefined;
  return dependency.endsWith(".ARCH")
    ? `${dependency.slice(0, -5)}.${platform}`
    : dependency;
}

export function resolveClosure(packages, roots, platform) {
  const pending = [...roots];
  const selected = new Set();
  const missing = new Set();

  while (pending.length > 0) {
    const name = pending.pop();
    if (selected.has(name)) continue;
    const entry = packages.get(name);
    if (!entry) {
      missing.add(name);
      continue;
    }

    selected.add(name);
    for (const dependency of entry.dependencies) {
      const concrete = concreteDependency(dependency, platform);
      if (concrete) pending.push(concrete);
    }
  }
  return {
    missing: [...missing].sort(),
    selected: [...selected].sort(),
  };
}

export function measureClosure(packages, roots, platform) {
  const { missing, selected } = resolveClosure(packages, roots, platform);
  const archiveBytes = { runtime: 0, documentation: 0, source: 0 };
  const installedBytes = { runtime: 0, documentation: 0, source: 0 };
  const licenseCounts = new Map();
  let packagesWithoutLicense = 0;
  for (const name of selected) {
    const entry = packages.get(name);
    for (const kind of Object.keys(archiveBytes)) {
      archiveBytes[kind] += entry.archiveBytes[kind];
      // TLPDB file sizes are counts of 4 KiB blocks.
      installedBytes[kind] += entry.installedBlocks[kind] * 4096;
    }
    if (entry.license) {
      licenseCounts.set(
        entry.license,
        (licenseCounts.get(entry.license) ?? 0) + 1,
      );
    } else {
      packagesWithoutLicense += 1;
    }
  }

  return {
    roots,
    platform,
    packageCount: selected.length,
    archiveBytes,
    installedBytes,
    licenseCounts: Object.fromEntries([...licenseCounts].sort()),
    packagesWithoutLicense,
    missing,
  };
}

function parseArguments(arguments_) {
  const result = {
    platform: "x86_64-linux",
    roots: DEFAULT_ROOTS,
  };

  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (argument === "--platform") {
      result.platform = arguments_.at(++index);
    } else if (argument === "--roots") {
      result.roots = arguments_.at(++index)?.split(",").filter(Boolean);
    } else if (!result.tlpdb) {
      result.tlpdb = argument;
    } else {
      throw new Error(`Unexpected argument: ${argument}`);
    }
  }

  if (!result.tlpdb || !result.platform || !result.roots?.length) {
    throw new Error(
      "Usage: node scripts/measure-texlive.mjs <texlive.tlpdb> " +
        "[--platform x86_64-linux] [--roots package,collection]",
    );
  }
  return result;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const source = await readFile(options.tlpdb, "utf8");
  const measurement = measureClosure(
    parseTlpdb(source),
    options.roots,
    options.platform,
  );
  process.stdout.write(`${JSON.stringify(measurement, undefined, 2)}\n`);
  if (measurement.missing.length > 0) process.exitCode = 2;
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  await main();
}
