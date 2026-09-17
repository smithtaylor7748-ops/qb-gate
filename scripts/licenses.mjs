import { readFileSync, readdirSync, existsSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { execFileSync } from "node:child_process";
const supplements = JSON.parse(
  readFileSync("docs/license-supplements.json", "utf8"),
);
const inventory = [];
const notices = [];
function add(item, dir) {
  const names = existsSync(dir)
    ? readdirSync(dir).filter((n) =>
        /^(?:licen[cs]e|copying|copyright|notice|unlicen[cs]e)(?:$|[._-])/i.test(
          n,
        ),
      )
    : [];
  const texts = [];
  for (const name of names) {
    try {
      const text = readFileSync(join(dir, name), "utf8");
      if (text.length < 300000) texts.push(`${name}\n${text}`);
    } catch {}
  }
  for (const entry of supplements[
    `${item.ecosystem}:${item.name}@${item.version}`
  ] ?? []) {
    names.push(entry.source);
    texts.push(`${entry.source}\n${entry.text}`);
  }
  inventory.push({ ...item, licenseFiles: names });
  notices.push(
    `${item.ecosystem}: ${item.name}@${item.version}\nSource: ${item.source}\nDeclared license: ${item.license}\n${texts.length ? texts.join("\n\n") : "License text is not present in this package installation; consult the fixed upstream package source."}`,
  );
}
const lock = JSON.parse(readFileSync("package-lock.json", "utf8"));
for (const [path, item] of Object.entries(lock.packages)) {
  if (!path || !existsSync(join(path, "package.json"))) continue;
  const p = JSON.parse(readFileSync(join(path, "package.json"), "utf8"));
  add(
    {
      ecosystem: "npm",
      name: p.name,
      version: p.version,
      license:
        typeof p.license === "string"
          ? p.license
          : (p.license?.type ?? "See upstream"),
      source: item.resolved ?? p.repository?.url ?? "",
      development: !!item.dev,
    },
    path,
  );
}
const metadata = JSON.parse(
  execFileSync(
    "cargo",
    [
      "metadata",
      "--locked",
      "--format-version",
      "1",
      "--manifest-path",
      "src-tauri/Cargo.toml",
    ],
    { encoding: "utf8", maxBuffer: 32 * 1024 * 1024, windowsHide: true },
  ),
);
for (const p of metadata.packages) {
  if (p.name === "qb-gate") continue;
  add(
    {
      ecosystem: "cargo",
      name: p.name,
      version: p.version,
      license: p.license ?? "See upstream license file",
      source: p.repository ?? `https://crates.io/crates/${p.name}/${p.version}`,
    },
    dirname(p.manifest_path),
  );
}
inventory.sort((a, b) =>
  `${a.ecosystem}/${a.name}/${a.version}`.localeCompare(
    `${b.ecosystem}/${b.name}/${b.version}`,
  ),
);
writeFileSync(
  "docs/dependencies.json",
  JSON.stringify(
    {
      schema: 1,
      scope:
        "Installed npm packages (including development dependencies) and Cargo lockfile dependency graph. Catalog extensions are separately documented in ATTRIBUTION.md.",
      packages: inventory,
    },
    null,
    2,
  ) + "\n",
);
writeFileSync(
  "THIRD_PARTY_NOTICES.txt",
  "QB Gate third-party dependency notices\nGenerated from package-lock.json and Cargo.lock. Includes build and test dependencies as well as runtime dependencies.\nThe project license is AGPL-3.0-only; each dependency retains its own license.\n\n" +
    notices.sort().join("\n\n" + "=".repeat(72) + "\n\n"),
);
console.log(
  `Recorded ${inventory.length} dependencies and bundled available license texts.`,
);
