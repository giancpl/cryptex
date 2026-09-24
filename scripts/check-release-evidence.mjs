import { createHash } from "node:crypto";
import { lstat, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const REQUIRED_CHECKS = Object.freeze([
  "cleanInstall",
  "offlineCompile",
  "upgrade",
  "rollback",
  "uninstallPreservesProjects",
  "adversarialSuite",
  "acceptanceSuite",
  "portability",
  "licenseReview",
  "sandboxReview",
  "signing",
  "reproducibility",
]);

const REQUIRED_ARTIFACTS = Object.freeze([
  "appimage",
  "deb",
  "texlive-inventory",
  "texlive-notices",
  "texlive-payload",
]);

const SHA256 = /^[a-f0-9]{64}$/u;
const COMMIT = /^[a-f0-9]{40}$/u;
const VERSION = /^0\.1\.0(?:-[0-9A-Za-z.-]+)?$/u;

export async function validateReleaseEvidence(
  evidence,
  { baseDirectory, expectedCommit } = {},
) {
  const errors = [];
  if (!evidence || typeof evidence !== "object" || Array.isArray(evidence)) {
    return ["release evidence must be a JSON object"];
  }
  if (evidence.schemaVersion !== 1) errors.push("schemaVersion must be 1");
  if (!VERSION.test(evidence.releaseVersion ?? "")) {
    errors.push("releaseVersion must be a 0.1.0 release identifier");
  }
  if (!COMMIT.test(evidence.commit ?? "")) {
    errors.push("commit must be a lowercase 40-character Git SHA");
  } else if (expectedCommit && evidence.commit !== expectedCommit) {
    errors.push(`commit does not match HEAD (${expectedCommit})`);
  }
  if (evidence.target !== "ubuntu-24.04-x86_64") {
    errors.push("target must be ubuntu-24.04-x86_64");
  }
  if (
    typeof evidence.generatedAt !== "string" ||
    !Number.isFinite(Date.parse(evidence.generatedAt))
  ) {
    errors.push("generatedAt must be an ISO-8601 timestamp");
  }

  const artifacts = Array.isArray(evidence.artifacts) ? evidence.artifacts : [];
  if (!Array.isArray(evidence.artifacts))
    errors.push("artifacts must be an array");
  const seenKinds = new Set();
  for (const artifact of artifacts) {
    const label = artifact?.kind ?? "<unknown>";
    if (!REQUIRED_ARTIFACTS.includes(label)) {
      errors.push(`artifact has unsupported kind: ${label}`);
      continue;
    }
    if (seenKinds.has(label))
      errors.push(`artifact kind is duplicated: ${label}`);
    seenKinds.add(label);
    if (!SHA256.test(artifact.sha256 ?? "")) {
      errors.push(`${label} has an invalid SHA-256 digest`);
    }
    const resolved = resolveEvidencePath(
      baseDirectory,
      artifact.path,
      label,
      errors,
    );
    if (resolved) {
      try {
        const metadata = await lstat(resolved);
        if (!metadata.isFile()) {
          errors.push(`${label} path is not a regular file`);
        } else if (SHA256.test(artifact.sha256 ?? "")) {
          const actual = createHash("sha256")
            .update(await readFile(resolved))
            .digest("hex");
          if (actual !== artifact.sha256) {
            errors.push(`${label} digest does not match its file`);
          }
        }
      } catch {
        errors.push(`${label} file is missing or unreadable`);
      }
    }
  }
  for (const kind of REQUIRED_ARTIFACTS) {
    if (!seenKinds.has(kind))
      errors.push(`required artifact is missing: ${kind}`);
  }

  const checks =
    evidence.checks && typeof evidence.checks === "object"
      ? evidence.checks
      : {};
  if (!evidence.checks || Array.isArray(evidence.checks)) {
    errors.push("checks must be an object");
  }
  for (const name of REQUIRED_CHECKS) {
    const check = checks[name];
    if (check?.status !== "passed") {
      errors.push(`${name} must have status passed`);
    }
    const resolved = resolveEvidencePath(
      baseDirectory,
      check?.evidence,
      `${name} evidence`,
      errors,
    );
    if (resolved) {
      try {
        if (!(await lstat(resolved)).isFile()) {
          errors.push(`${name} evidence is not a regular file`);
        }
      } catch {
        errors.push(`${name} evidence is missing or unreadable`);
      }
    }
  }
  for (const name of Object.keys(checks)) {
    if (!REQUIRED_CHECKS.includes(name)) {
      errors.push(`unsupported release check: ${name}`);
    }
  }
  return errors;
}

function resolveEvidencePath(baseDirectory, candidate, label, errors) {
  if (typeof candidate !== "string" || candidate.length === 0) {
    errors.push(`${label} path is required`);
    return undefined;
  }
  if (path.isAbsolute(candidate)) {
    errors.push(`${label} path must be relative to the evidence directory`);
    return undefined;
  }
  const base = path.resolve(baseDirectory ?? ".");
  const resolved = path.resolve(base, candidate);
  if (resolved === base || !resolved.startsWith(`${base}${path.sep}`)) {
    errors.push(`${label} path escapes the evidence directory`);
    return undefined;
  }
  return resolved;
}

async function main() {
  const index = process.argv.indexOf("--evidence");
  const evidenceArgument = index >= 0 ? process.argv[index + 1] : undefined;
  if (!evidenceArgument) {
    throw new Error("usage: pnpm release:validate -- --evidence FILE");
  }
  const evidencePath = path.resolve(evidenceArgument);
  const evidence = JSON.parse(await readFile(evidencePath, "utf8"));
  const expectedCommit = process.env.CRYPTEX_RELEASE_COMMIT;
  const errors = await validateReleaseEvidence(evidence, {
    baseDirectory: path.dirname(evidencePath),
    expectedCommit,
  });
  if (errors.length > 0) {
    throw new Error(`release gate failed:\n- ${errors.join("\n- ")}`);
  }
  process.stdout.write(
    `Release evidence valid for ${evidence.releaseVersion} (${evidence.commit})\n`,
  );
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await main();
}
