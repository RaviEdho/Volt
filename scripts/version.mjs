import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
const versionPattern = /^(0|[1-9]\d{0,4})\.(0|[1-9]\d{0,4})\.(0|[1-9]\d{0,4})$/;
const validVersion = value => typeof value === "string" && versionPattern.test(value)
  && value.split(".").every(part => Number(part) <= 65535);
const json = path => {
  const file = fileURLToPath(new URL(path, root));
  const data = JSON.parse(readFileSync(file, "utf8"));
  return { data, save: () => writeFileSync(file, `${JSON.stringify(data, null, 2)}\n`) };
};
const toml = (path, pattern) => {
  const file = fileURLToPath(new URL(path, root));
  const text = readFileSync(file, "utf8");
  const matches = [...text.matchAll(pattern)];
  if (matches.length !== 1) throw new Error(`Could not locate exactly one Volt version in ${path}`);
  return {
    version: matches[0][2],
    save: version => writeFileSync(file, text.replace(pattern, (_, before, old, after) => `${before}${version}${after}`)),
  };
};

function run([command, argument, ...extra]) {
  if (extra.length || !["set", "check"].includes(command)) {
    throw new Error("Usage: version.mjs set X.Y.Z | check [vX.Y.Z]");
  }
  const npm = json("package.json");
  const npmLock = json("package-lock.json");
  const tauri = json("src-tauri/tauri.conf.json");
  const cargo = toml("src-tauri/Cargo.toml", /(^\[package\]\r?\nname = "volt"\r?\nversion = ")([^"\r\n]+)(")/gm);
  const cargoLock = toml("src-tauri/Cargo.lock", /(^\[\[package\]\]\r?\nname = "volt"\r?\nversion = ")([^"\r\n]+)(")/gm);
  const lockedPackage = npmLock.data.packages?.[""];
  if (npm.data.name !== "volt" || lockedPackage?.name !== "volt") {
    throw new Error("Could not locate the Volt package in the npm manifest and lockfile.");
  }
  if (command === "set") {
    if (!validVersion(argument)) throw new Error("Enter a stable X.Y.Z version (each part must be 0–65535).");
    npm.data.version = npmLock.data.version = lockedPackage.version = tauri.data.version = argument;
    npm.save(); npmLock.save(); tauri.save(); cargo.save(argument); cargoLock.save(argument);
    console.log(`Set Volt to ${argument}. Commit the version files, then tag v${argument} to create a release.`);
    return;
  }
  const expected = npm.data.version;
  if (!validVersion(expected)) throw new Error(`Invalid Volt version in package.json: ${expected}`);
  for (const [path, version] of Object.entries({
    "package-lock.json": npmLock.data.version,
    "package-lock.json root package": lockedPackage.version,
    "tauri.conf.json": tauri.data.version,
    "Cargo.toml": cargo.version,
    "Cargo.lock": cargoLock.version,
  })) {
    if (version !== expected) throw new Error(`${path}: expected ${expected}, found ${version}.`);
  }
  if (argument !== undefined && argument !== `v${expected}`) {
    throw new Error(`Tag ${argument} does not match app version v${expected}.`);
  }
  console.log(`Volt version ${expected} is consistent.`);
}

try {
  run(process.argv.slice(2));
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
