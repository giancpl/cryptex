#!/usr/bin/env node
import { access, cp, mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { isAbsolute, join, relative, resolve, sep } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { constants } from "node:fs";

export const REQUIRED_FIXTURES = [
  "minimal",
  "multipass",
  "bibtex",
  "biber",
  "cryptocode",
  "tikz",
  "custom-sty",
  "xelatex",
  "lualatex",
  "subfiles",
  "magic-root",
  "multi-root",
  "broken-command",
  "broken-reference",
  "broken-citation",
  "malformed-log",
  "latexmkrc-safe",
  "latexmkrc-exec",
  "shell-escape",
  "symlink-inside",
  "symlink-outside",
  "symlink-loop",
  "external-edit",
  "large-project",
  "unicode-paths",
  "non-utf8",
  "binary-file",
  "malformed-pdf",
  "large-pdf",
  "overleaf-portable",
  "notation-positive",
  "notation-negative",
];

function safeRelative(value, label) {
  if (
    typeof value !== "string" ||
    !value ||
    isAbsolute(value) ||
    value.split(/[\\/]/).includes("..")
  )
    throw new Error(`${label} must be a safe non-empty relative path`);
  return value;
}

async function regular(path, label) {
  const value = await stat(path).catch(() => null);
  if (!value?.isFile())
    throw new Error(`${label} is missing or not a regular file`);
}

export async function loadManifest(path) {
  return JSON.parse(await readFile(path, "utf8"));
}

export async function validateFixtureSet(manifest, fixturesRoot) {
  if (manifest.schemaVersion !== 1 || !Array.isArray(manifest.fixtures))
    throw new Error("unsupported acceptance manifest");
  const ids = manifest.fixtures.map((fixture) => fixture.id);
  if (new Set(ids).size !== ids.length) throw new Error("duplicate fixture id");
  for (const id of REQUIRED_FIXTURES)
    if (!ids.includes(id)) throw new Error(`missing required fixture: ${id}`);
  for (const fixture of manifest.fixtures) {
    safeRelative(fixture.id, "fixture id");
    safeRelative(fixture.root, `${fixture.id} root`);
    if (!["compile", "expectedFailure", "static"].includes(fixture.mode))
      throw new Error(`${fixture.id} has invalid mode`);
    const directory = resolve(fixturesRoot, fixture.id);
    if (relative(resolve(fixturesRoot), directory).split(sep).includes(".."))
      throw new Error(`${fixture.id} escapes fixture root`);
    await regular(
      join(directory, fixture.root),
      `${fixture.id}/${fixture.root}`,
    );
    for (const path of fixture.requiredFiles ?? []) {
      safeRelative(path, `${fixture.id} required file`);
      await regular(join(directory, path), `${fixture.id}/${path}`);
    }
    if (
      fixture.mode !== "static" &&
      !["pdf", "xelatex", "lualatex"].includes(fixture.engine)
    )
      throw new Error(`${fixture.id} has invalid engine`);
    if (fixture.mode === "expectedFailure" && !fixture.expectedLog)
      throw new Error(`${fixture.id} must declare expectedLog`);
  }
  return { fixtures: manifest.fixtures.length };
}

function latexmkArgs(fixture) {
  const engine =
    fixture.engine === "xelatex"
      ? "-xelatex"
      : fixture.engine === "lualatex"
        ? "-lualatex"
        : "-pdf";
  return [
    "-norc",
    engine,
    "-interaction=nonstopmode",
    "-file-line-error",
    "-synctex=1",
    fixture.root,
  ];
}

export async function runCompilableFixtures(manifest, fixturesRoot, texBin) {
  if (!isAbsolute(texBin))
    throw new Error("managed TeX bin path must be absolute");
  for (const executable of [
    "latexmk",
    "pdflatex",
    "xelatex",
    "lualatex",
    "bibtex",
    "biber",
    "synctex",
  ]) {
    const executablePath = join(texBin, executable);
    await regular(executablePath, "managed executable " + executable);
    await access(executablePath, constants.X_OK);
  }
  const workspace = await mkdtemp(join(tmpdir(), "cryptex-acceptance-"));
  const results = [];
  try {
    for (const fixture of manifest.fixtures.filter(
      (item) => item.mode !== "static",
    )) {
      const cwd = join(workspace, fixture.id);
      await cp(join(fixturesRoot, fixture.id), cwd, { recursive: true });
      const result = spawnSync(join(texBin, "latexmk"), latexmkArgs(fixture), {
        cwd,
        encoding: "utf8",
        shell: false,
        env: {
          PATH: `${texBin}:/usr/bin:/bin`,
          SOURCE_DATE_EPOCH: "0",
          LANG: "C.UTF-8",
          LC_ALL: "C.UTF-8",
          TZ: "UTC",
        },
        maxBuffer: 8 * 1024 * 1024,
      });
      const output = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
      if (fixture.mode === "expectedFailure") {
        if (result.status === 0)
          throw new Error(`${fixture.id} unexpectedly compiled`);
        if (!output.includes(fixture.expectedLog))
          throw new Error(
            `${fixture.id} lacked expected log evidence: ${fixture.expectedLog}`,
          );
      } else {
        if (result.status !== 0)
          throw new Error(
            `${fixture.id} failed with exit ${result.status}\n${output.slice(-2000)}`,
          );
        for (const artifact of fixture.expectedArtifacts ?? [])
          await regular(
            join(cwd, artifact),
            `${fixture.id} artifact ${artifact}`,
          );
      }
      results.push({
        id: fixture.id,
        outcome:
          fixture.mode === "expectedFailure" ? "expectedFailure" : "passed",
      });
    }
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
  return results;
}

async function main() {
  const here = resolve(fileURLToPath(new URL("..", import.meta.url)));
  const manifestPath = join(here, "tests", "acceptance", "manifest.json");
  const fixturesRoot = join(here, "tests", "acceptance", "fixtures");
  const manifest = await loadManifest(manifestPath);
  const validation = await validateFixtureSet(manifest, fixturesRoot);
  const compileAt = process.argv.indexOf("--compile");
  if (compileAt >= 0) {
    const texBin = process.argv[compileAt + 1];
    if (!texBin)
      throw new Error("--compile requires an absolute managed TeX bin path");
    const results = await runCompilableFixtures(manifest, fixturesRoot, texBin);
    console.log(JSON.stringify({ ...validation, compiled: results }, null, 2));
  } else {
    console.log(JSON.stringify({ ...validation, compiled: [] }, null, 2));
  }
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
