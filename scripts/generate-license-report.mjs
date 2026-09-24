#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

const JSON_OUTPUT = new URL("../licenses/dependencies.json", import.meta.url);
const MARKDOWN_OUTPUT = new URL("../THIRD_PARTY_NOTICES.md", import.meta.url);

export function normalizeNpmLicenses(grouped) {
  return Object.entries(grouped)
    .flatMap(([groupLicense, entries]) =>
      entries.flatMap((entry) =>
        entry.versions.map((version) => ({
          license: entry.license || groupLicense,
          name: entry.name,
          version,
        })),
      ),
    )
    .sort(compareDependency);
}

export function normalizeCargoMetadata(metadata) {
  return metadata.packages
    .filter((entry) => entry.source?.startsWith("registry+"))
    .map((entry) => {
      if (!entry.license) {
        throw new Error(
          `Rust dependency ${entry.name}@${entry.version} has no SPDX license expression`,
        );
      }
      return {
        license: entry.license,
        name: entry.name,
        version: entry.version,
      };
    })
    .sort(compareDependency);
}

export function createLicenseReport(npm, cargo) {
  return {
    formatVersion: 1,
    applicationLicense: "GPL-3.0-or-later",
    generatedFrom: {
      npm: "pnpm-lock.yaml production dependency graph",
      rust: "src-tauri/Cargo.lock registry dependency graph",
    },
    npm,
    rust: cargo,
  };
}

export function renderNotices(report) {
  const rows = [
    "# Third-party dependency inventory",
    "",
    "CrypTex application code is GPL-3.0-or-later. The packages below retain",
    "their respective licenses. This generated inventory does not cover the separate",
    "managed TeX Live payload, system libraries, or replace required license texts.",
    "",
    "## npm production dependencies",
    "",
    "| Package | Version | License |",
    "| --- | --- | --- |",
    ...report.npm.map(
      (entry) =>
        `| ${escapeCell(entry.name)} | ${escapeCell(entry.version)} | ${escapeCell(entry.license)} |`,
    ),
    "",
    "## Rust registry dependencies",
    "",
    "| Crate | Version | License |",
    "| --- | --- | --- |",
    ...report.rust.map(
      (entry) =>
        `| ${escapeCell(entry.name)} | ${escapeCell(entry.version)} | ${escapeCell(entry.license)} |`,
    ),
    "",
  ];
  return `${rows.join("\n")}\n`;
}

function compareDependency(left, right) {
  return (
    left.name.localeCompare(right.name, "en") ||
    left.version.localeCompare(right.version, "en") ||
    left.license.localeCompare(right.license, "en")
  );
}

function escapeCell(value) {
  return String(value).replaceAll("|", "\\|").replaceAll("\n", " ");
}

function commandJson(command, arguments_) {
  return JSON.parse(
    execFileSync(command, arguments_, {
      cwd: new URL("..", import.meta.url),
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      stdio: ["ignore", "pipe", "inherit"],
    }),
  );
}

async function atomicWrite(url, contents) {
  const temporary = new URL(`${url.pathname}.tmp-${process.pid}`, "file://");
  await writeFile(temporary, contents, { encoding: "utf8", flag: "wx" });
  await rename(temporary, url);
}

async function main() {
  const check = process.argv.slice(2).includes("--check");
  const npm = normalizeNpmLicenses(
    commandJson("pnpm", ["licenses", "list", "--json", "--prod"]),
  );
  const cargo = normalizeCargoMetadata(
    commandJson("cargo", [
      "metadata",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--format-version",
      "1",
      "--locked",
    ]),
  );
  const report = createLicenseReport(npm, cargo);
  const json = `${JSON.stringify(report, undefined, 2)}\n`;
  const markdown = renderNotices(report);
  if (check) {
    const [existingJson, existingMarkdown] = await Promise.all([
      readFile(JSON_OUTPUT, "utf8"),
      readFile(MARKDOWN_OUTPUT, "utf8"),
    ]);
    if (existingJson !== json || existingMarkdown !== markdown) {
      throw new Error(
        "dependency license inventory drifted; run pnpm licenses:generate",
      );
    }
  } else {
    await mkdir(new URL("../licenses/", import.meta.url), { recursive: true });
    await Promise.all([
      atomicWrite(JSON_OUTPUT, json),
      atomicWrite(MARKDOWN_OUTPUT, markdown),
    ]);
  }
  process.stdout.write(
    `License inventory valid: ${npm.length} npm and ${cargo.length} Rust dependencies\n`,
  );
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  await main();
}
