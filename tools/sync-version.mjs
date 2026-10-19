import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error("用法: node tools/sync-version.mjs 0.2.0");
  process.exit(1);
}

const pkg = JSON.parse(readFileSync("package.json", "utf8"));
const previous = pkg.version;
pkg.version = version;
writeFileSync("package.json", `${JSON.stringify(pkg, null, 2)}\n`);

const lock = JSON.parse(readFileSync("package-lock.json", "utf8"));
lock.version = version;
if (lock.packages?.[""]) {
  lock.packages[""].version = version;
}
writeFileSync("package-lock.json", `${JSON.stringify(lock, null, 2)}\n`);

let cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
cargo = cargo.replace(/^version = "[^"]+"/m, `version = "${version}"`);
writeFileSync("src-tauri/Cargo.toml", cargo);

const conf = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
conf.version = version;
writeFileSync(
  "src-tauri/tauri.conf.json",
  `${JSON.stringify(conf, null, 2)}\n`,
);

const cargoLockPath = "src-tauri/Cargo.lock";
let cargoLock = readFileSync(cargoLockPath, "utf8");
cargoLock = cargoLock.replace(
  /(\[\[package\]\]\nname = "startupmanager"\nversion = ")[^"]+(")/,
  `$1${version}$2`,
);
writeFileSync(cargoLockPath, cargoLock);

const readmePath = "README.md";
let readme = readFileSync(readmePath, "utf8");
readme = readme.replaceAll(
  `StartupManager_${previous}_x64-setup.exe`,
  `StartupManager_${version}_x64-setup.exe`,
);
writeFileSync(readmePath, readme);

console.log(
  `版本已同步为 ${version}（package.json / package-lock.json / Cargo.toml / Cargo.lock / tauri.conf.json / README.md）`,
);
