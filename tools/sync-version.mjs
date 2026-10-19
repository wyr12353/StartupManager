import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const filePath = (relative) => join(root, relative);
const read = (relative) => readFileSync(filePath(relative), "utf8");
const results = [];

function writeIfChanged(relative, next) {
  const file = filePath(relative);
  const current = readFileSync(file, "utf8");
  if (current === next) {
    results.push(["unchanged", relative]);
    return;
  }
  writeFileSync(file, next);
  results.push(["updated", relative]);
}

function replaceSectionValue(text, section, key, value) {
  const sectionRe = new RegExp(`^\\[${section}\\][ \\t]*\\r?$`, "m");
  const sectionMatch = sectionRe.exec(text);
  if (!sectionMatch) {
    throw new Error(`找不到 [${section}] 段`);
  }
  const start = sectionMatch.index;
  const bodyStart = start + sectionMatch[0].length;
  const nextSection = text.slice(bodyStart).search(/^\[/m);
  const end = nextSection === -1 ? text.length : bodyStart + nextSection;
  const body = text.slice(start, end);
  const keyRe = new RegExp(`(^[ \\t]*${key}[ \\t]*=[ \\t]*")[^"]+(")`, "m");
  if (!keyRe.test(body)) {
    throw new Error(`[${section}] 段内找不到 ${key}`);
  }
  return text.slice(0, start) + body.replace(keyRe, `$1${value}$2`) + text.slice(end);
}

function replaceCargoLockVersion(text, value) {
  const marker = /^\[\[package\]\][ \t]*\r?$/gm;
  const starts = [];
  let match;
  while ((match = marker.exec(text)) !== null) {
    starts.push(match.index);
  }
  for (let index = 0; index < starts.length; index += 1) {
    const start = starts[index];
    const end = index + 1 < starts.length ? starts[index + 1] : text.length;
    const block = text.slice(start, end);
    if (!/^name[ \t]*=[ \t]*"startupmanager"[ \t]*\r?$/m.test(block)) {
      continue;
    }
    const versionRe = /(^version[ \t]*=[ \t]*")[^"]+(")/m;
    if (!versionRe.test(block)) {
      throw new Error("Cargo.lock 的 startupmanager 包块内找不到 version");
    }
    return text.slice(0, start) + block.replace(versionRe, `$1${value}$2`) + text.slice(end);
  }
  throw new Error("Cargo.lock 中找不到 startupmanager 包块");
}

function replaceReadmeVersions(text, value) {
  const artifactRe = /(?:启动项管理器|StartupManager)_\d+\.\d+\.\d+_x64(?:-setup\.exe|\.msi)/g;
  const matches = [...text.matchAll(artifactRe)];
  if (matches.length !== 3) {
    throw new Error(`README.md 期望 3 处产物文件名，实际 ${matches.length} 处`);
  }
  return text.replace(artifactRe, (match) =>
    match.replace(/_\d+\.\d+\.\d+(_x64(?:-setup\.exe|\.msi))/, `_${value}$1`),
  );
}

function replaceAppVersion(text, value) {
  const re = /(appState\?\.version \?\? ')[^']+(')/;
  if (!re.test(text)) {
    throw new Error("src/App.vue 中找不到 appState?.version ?? '...'");
  }
  return text.replace(re, `$1${value}$2`);
}

function main() {
  const requested = process.argv[2];
  const pkg = JSON.parse(read("package.json"));
  const previous = pkg.version;
  const version = requested ?? previous;

  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(`版本号不合法: ${version}`);
  }

  if (requested && version !== previous) {
    pkg.version = version;
    writeIfChanged("package.json", `${JSON.stringify(pkg, null, 2)}\n`);
  } else {
    results.push(["unchanged", "package.json"]);
  }

  const lock = JSON.parse(read("package-lock.json"));
  if (!lock.packages?.[""]) {
    throw new Error('package-lock.json 缺少 packages[""]，请先运行 npm install --package-lock-only');
  }
  lock.version = version;
  lock.packages[""].version = version;
  writeIfChanged("package-lock.json", `${JSON.stringify(lock, null, 2)}\n`);

  writeIfChanged(
    "src-tauri/Cargo.toml",
    replaceSectionValue(read("src-tauri/Cargo.toml"), "package", "version", version),
  );
  writeIfChanged(
    "src-tauri/Cargo.lock",
    replaceCargoLockVersion(read("src-tauri/Cargo.lock"), version),
  );

  const tauri = JSON.parse(read("src-tauri/tauri.conf.json"));
  tauri.version = version;
  writeIfChanged("src-tauri/tauri.conf.json", `${JSON.stringify(tauri, null, 2)}\n`);

  writeIfChanged("README.md", replaceReadmeVersions(read("README.md"), version));
  writeIfChanged("src/App.vue", replaceAppVersion(read("src/App.vue"), version));

  for (const [state, file] of results) {
    console.log(`${state} ${file}`);
  }
  console.log(`版本同步完成: ${previous} -> ${version}`);

  const check = spawnSync(process.execPath, [filePath("tools/check-version.mjs")], {
    stdio: "inherit",
  });
  process.exitCode = check.status ?? 1;
}

try {
  main();
} catch (error) {
  console.error(`版本同步失败: ${error.message}`);
  process.exitCode = 1;
}
