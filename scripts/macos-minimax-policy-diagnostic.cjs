// Native CI failure diagnostics only: no package installation or script execution.
const fs = require("node:fs");
const path = require("node:path");

if (process.platform !== "darwin" || process.env.GITHUB_ACTIONS !== "true") {
  throw new Error("Requires a disposable GitHub macOS runner");
}
const [npmProgram, prefix] = process.argv.slice(2);
if (!prefix || !path.isAbsolute(prefix) || !prefix.startsWith(process.env.HOME + path.sep)) {
  throw new Error("Requires the isolated fixture prefix");
}
const npmRoot = path.dirname(path.dirname(fs.realpathSync(npmProgram)));
const Npm = require(path.join(npmRoot, "lib/npm.js"));
const resolvePolicy = require(path.join(npmRoot, "lib/utils/resolve-allow-scripts.js"));
const pkgJson = require(path.join(npmRoot, "node_modules/@npmcli/package-json"));
const Arborist = require(path.join(npmRoot, "node_modules/@npmcli/arborist"));
const allowed = require(path.join(npmRoot, "node_modules/@npmcli/arborist/lib/script-allowed.js"));
const names = ["@minimax-ai/code", "better-sqlite3"];
const safeUrl = (value) => {
  if (!value) return value;
  try {
    const url = new URL(value);
    url.username = "";
    url.password = "";
    url.search = "";
    url.hash = "";
    return url.toString();
  } catch {
    return value.startsWith("file:") ? "file dependency" : "non-URL dependency";
  }
};
async function diagnose(label) {
  process.argv = [process.execPath, __filename, "install", "--prefix", prefix,
    "--global=false", "--no-audit", "--no-fund", "--engine-strict",
    "--include=optional", "--ignore-scripts=false", "@minimax-ai/code@latest",
    "--strict-allow-scripts"];
  const npm = new Npm();
  await npm.config.load();
  const resolved = await resolvePolicy(npm);
  console.log("npm-policy-context", JSON.stringify({label, cwd:process.cwd(),
    canonicalCwd:fs.realpathSync(process.cwd()), canonicalPrefix:fs.realpathSync(npm.prefix),
    prefix:npm.prefix, localPrefix:npm.localPrefix, global:npm.global,
    source:resolved.source, policy:resolved.policy}));
  try {
    const {content} = await pkgJson.normalize(npm.prefix);
    console.log("npm-normalized-policy", JSON.stringify({label, policy:content.allowScripts}));
  } catch (error) {
    console.log("npm-normalize-error", JSON.stringify({label, code:error.code, message:error.message}));
  }
  for (const treeKind of ["virtual", "actual", "ideal"]) {
    try {
      const arb = new Arborist({...npm.flatOptions, auditLevel:null, path:npm.prefix,
        add:["@minimax-ai/code@latest"], allowScripts:resolved.policy, offline:true});
      console.log("npm-arborist-policy", JSON.stringify({label, treeKind, policy:arb.options.allowScripts}));
      const tree = treeKind === "virtual" ? await arb.loadVirtual()
        : treeKind === "actual" ? await arb.loadActual() : await arb.buildIdealTree();
      for (const node of tree.inventory.values()) {
        if (!names.includes(node.name)) continue;
        console.log("npm-policy-node", JSON.stringify({label, treeKind, name:node.name,
          version:node.version, resolved:safeUrl(node.resolved), inBundle:node.inBundle,
          isRegistryDependency:node.isRegistryDependency, inert:node.inert,
          edges:[...node.edgesIn].map(edge => ({name:edge.name,
            spec:/^https?:/.test(edge.spec) ? safeUrl(edge.spec) : edge.spec, type:edge.type})),
          trusted:allowed.getTrustedRegistryIdentity(node), verdict:allowed(node,resolved.policy)}));
      }
    } catch (error) {
      console.log("npm-tree-error", JSON.stringify({label, treeKind, code:error.code, message:error.message}));
    }
  }
}
(async () => {
  await diagnose("original-cwd");
  process.chdir(prefix);
  await diagnose("prefix-cwd");
})().catch(error => { console.error(error.message); process.exitCode = 1; });
