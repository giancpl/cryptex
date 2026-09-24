import { createHash } from "node:crypto";
import { mkdtemp, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  REQUIRED_CHECKS,
  validateReleaseEvidence,
} from "./check-release-evidence.mjs";

async function fixture() {
  const directory = await mkdtemp(path.join(tmpdir(), "cryptex-release-"));
  const artifacts = [];
  for (const kind of [
    "appimage",
    "deb",
    "texlive-inventory",
    "texlive-notices",
    "texlive-payload",
  ]) {
    const file = `${kind}.data`;
    const contents = Buffer.from(kind);
    await writeFile(path.join(directory, file), contents);
    artifacts.push({
      kind,
      path: file,
      sha256: createHash("sha256").update(contents).digest("hex"),
    });
  }
  const checks = {};
  for (const name of REQUIRED_CHECKS) {
    const file = `${name}.txt`;
    await writeFile(path.join(directory, file), `${name} passed\n`);
    checks[name] = { status: "passed", evidence: file };
  }
  return {
    directory,
    evidence: {
      schemaVersion: 1,
      releaseVersion: "0.1.0-rc.1",
      commit: "a".repeat(40),
      target: "ubuntu-24.04-x86_64",
      generatedAt: "2026-09-24T10:00:00.000Z",
      artifacts,
      checks,
    },
  };
}

describe("release evidence gate", () => {
  it("accepts a complete matching evidence directory", async () => {
    const { directory, evidence } = await fixture();
    await expect(
      validateReleaseEvidence(evidence, {
        baseDirectory: directory,
        expectedCommit: "a".repeat(40),
      }),
    ).resolves.toEqual([]);
  });

  it("fails closed on missing checks, tampering, and a stale commit", async () => {
    const { directory, evidence } = await fixture();
    delete evidence.checks.signing;
    evidence.artifacts[0].sha256 = "0".repeat(64);
    const errors = await validateReleaseEvidence(evidence, {
      baseDirectory: directory,
      expectedCommit: "b".repeat(40),
    });
    expect(errors).toContain(`commit does not match HEAD (${"b".repeat(40)})`);
    expect(errors).toContain("appimage digest does not match its file");
    expect(errors).toContain("signing must have status passed");
  });

  it("rejects absolute, escaping, duplicate, and unknown evidence paths", async () => {
    const { directory, evidence } = await fixture();
    evidence.artifacts[0].path = "/tmp/outside";
    evidence.artifacts[1].kind = "appimage";
    evidence.checks.cleanInstall.evidence = "../outside";
    evidence.checks.unexpected = {
      status: "passed",
      evidence: "cleanInstall.txt",
    };
    const errors = await validateReleaseEvidence(evidence, {
      baseDirectory: directory,
    });
    expect(errors).toContain(
      "appimage path must be relative to the evidence directory",
    );
    expect(errors).toContain("artifact kind is duplicated: appimage");
    expect(errors).toContain("required artifact is missing: deb");
    expect(errors).toContain(
      "cleanInstall evidence path escapes the evidence directory",
    );
    expect(errors).toContain("unsupported release check: unexpected");
  });

  it("does not accept symlinks as artifacts or evidence", async () => {
    const { directory, evidence } = await fixture();
    await symlink("appimage.data", path.join(directory, "linked-appimage"));
    evidence.artifacts[0].path = "linked-appimage";
    await symlink("cleanInstall.txt", path.join(directory, "linked-evidence"));
    evidence.checks.cleanInstall.evidence = "linked-evidence";
    const errors = await validateReleaseEvidence(evidence, {
      baseDirectory: directory,
    });
    expect(errors).toContain("appimage path is not a regular file");
    expect(errors).toContain("cleanInstall evidence is not a regular file");
  });
});
