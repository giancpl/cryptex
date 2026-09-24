#!/usr/bin/env node
import { cp, mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { createHash } from "node:crypto";
import { basename, join, relative, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import {
  loadManifest,
  validateFixtureSet,
} from "./run-acceptance-fixtures.mjs";

export const PORTABLE_FIXTURES = [
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
  "overleaf-portable",
];

const TEXT_EXTENSIONS = new Set([".tex", ".bib", ".sty", ".cls"]);
const PROPRIETARY_NAMES = new Set([
  ".cryptex",
  ".cryptex.json",
  "cryptex.json",
]);

function extension(path) {
  const index = path.lastIndexOf(".");
  return index < 0 ? "" : path.slice(index).toLowerCase();
}

export function validatePortableFile(relativePath, bytes) {
  const name = basename(relativePath).toLowerCase();
  if (PROPRIETARY_NAMES.has(name) || name.startsWith(".cryptex-"))
    throw new Error(`proprietary project file is forbidden: ${relativePath}`);
  if (TEXT_EXTENSIONS.has(extension(relativePath))) {
    const text = bytes.toString("utf8");
    if (/\\cryptex[A-Za-z@]*/i.test(text))
      throw new Error(
        `proprietary LaTeX command is forbidden: ${relativePath}`,
      );
  }
}

async function files(directory, root = directory) {
  const found = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    const relativePath = relative(root, path);
    if (entry.isSymbolicLink())
      throw new Error(`portable fixture contains a symlink: ${relativePath}`);
    if (entry.isDirectory()) found.push(...(await files(path, root)));
    else if (entry.isFile()) found.push(relativePath);
    else
      throw new Error(
        `portable fixture contains a special file: ${relativePath}`,
      );
  }
  return found.sort();
}

function digest(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

export async function validatePortableFixtures(manifest, fixturesRoot) {
  const declared = manifest.fixtures
    .filter((fixture) => fixture.portable)
    .map((fixture) => fixture.id)
    .sort();
  const expected = [...PORTABLE_FIXTURES].sort();
  if (JSON.stringify(declared) !== JSON.stringify(expected))
    throw new Error(
      "portable fixture declaration does not match the required representative set",
    );

  const workspace = await mkdtemp(join(tmpdir(), "cryptex-portability-"));
  const results = [];
  try {
    for (const id of PORTABLE_FIXTURES) {
      const fixture = manifest.fixtures.find((item) => item.id === id);
      if (!fixture || fixture.mode !== "compile")
        throw new Error(`${id} is not a compilable portable fixture`);
      const source = resolve(fixturesRoot, id);
      const target = join(workspace, id);
      await cp(source, target, { recursive: true, verbatimSymlinks: true });
      const sourceFiles = await files(source);
      const copiedFiles = await files(target);
      if (JSON.stringify(sourceFiles) !== JSON.stringify(copiedFiles))
        throw new Error(`${id} changed while copied outside CrypTex`);
      for (const relativePath of sourceFiles) {
        const sourceBytes = await readFile(join(source, relativePath));
        const copiedBytes = await readFile(join(target, relativePath));
        validatePortableFile(relativePath, sourceBytes);
        if (digest(sourceBytes) !== digest(copiedBytes))
          throw new Error(`${id}/${relativePath} changed during copy`);
      }
      if (!copiedFiles.includes(fixture.root))
        throw new Error(`${id} copied root is missing`);
      results.push({ id, files: copiedFiles.length });
    }
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
  return results;
}

async function main() {
  const repository = resolve(fileURLToPath(new URL("..", import.meta.url)));
  const manifestPath = join(repository, "tests", "acceptance", "manifest.json");
  const fixturesRoot = join(repository, "tests", "acceptance", "fixtures");
  const manifest = await loadManifest(manifestPath);
  await validateFixtureSet(manifest, fixturesRoot);
  const portable = await validatePortableFixtures(manifest, fixturesRoot);
  console.log(JSON.stringify({ portable }, null, 2));
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
