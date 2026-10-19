import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const read = (relative) => readFileSync(join(root, relative), "utf8");
const expected = JSON.parse(read("package.json")).version;

const oks = [];
const failures = [];
const errors = [];

const lineOf = (text, index) => text.slice(0, index).split("\n").length;

function check(label, actual) {
  if (actual === undefined || actual === null) {
    errors.push({ label, message: "未找到版本载体" });
    return;
  }
  if (actual !== expected) {
    failures.push({ label, actual });
    return;
  }
  oks.push(label);
}

function findSectionValue(text, section, key) {
  const sectionRe = new RegExp(`^\\[${section}\\][ \\t]*\\r?$`, "m");
  const sectionMatch = sectionRe.exec(text);
  if (!sectionMatch) {
    return null;
  }
  const start = sectionMatch.index;
  const bodyStart = start + sectionMatch[0].length;
  const nextSection = text.slice(bodyStart).search(/^\[/m);
  const end = nextSection === -1 ? text.length : bodyStart + nextSection;
  const body = text.slice(start, end);
  const keyRe = new RegExp(`^[ \\t]*${key}[ \\t]*=[ \\t]*"([^"]+)"`, "m");
  const keyMatch = keyRe.exec(body);
  if (!keyMatch) {
    return null;
  }
  return {
    value: keyMatch[1],
    line: lineOf(text, start + keyMatch.index),
  };
}

function findCargoLockVersion(text) {
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
    const versionMatch = /^version[ \t]*=[ \t]*"([^"]+)"/m.exec(block);
    if (!versionMatch) {
      return null;
    }
    return {
      value: versionMatch[1],
      line: lineOf(text, start + versionMatch.index),
    };
  }
  return null;
}

console.log(`[version] 基准 package.json = ${expected}`);

const lockText = read("package-lock.json");
const lock = JSON.parse(lockText);
check("package-lock.json root version", lock.version);
check('package-lock.json packages[""] version', lock.packages?.[""]?.version);

const cargoToml = read("src-tauri/Cargo.toml");
const cargoVersion = findSectionValue(cargoToml, "package", "version");
if (cargoVersion) {
  check(`src-tauri/Cargo.toml:${cargoVersion.line} [package] version`, cargoVersion.value);
} else {
  errors.push({ label: "src-tauri/Cargo.toml [package] version", message: "未找到" });
}

const cargoLock = read("src-tauri/Cargo.lock");
const lockedVersion = findCargoLockVersion(cargoLock);
if (lockedVersion) {
  check(`src-tauri/Cargo.lock:${lockedVersion.line} startupmanager`, lockedVersion.value);
} else {
  errors.push({ label: "src-tauri/Cargo.lock startupmanager", message: "未找到包块" });
}

const tauri = JSON.parse(read("src-tauri/tauri.conf.json"));
check("src-tauri/tauri.conf.json version", tauri.version);

const readme = read("README.md");
const artifactRe = /(?:启动项管理器|StartupManager)_\d+\.\d+\.\d+_x64(?:-setup\.exe|\.msi)/g;
const artifacts = [...readme.matchAll(artifactRe)];
if (artifacts.length !== 3) {
  errors.push({
    label: "README.md artifacts",
    message: `期望 3 处产物文件名，实际 ${artifacts.length} 处`,
  });
} else {
  for (const artifact of artifacts) {
    const versionMatch = /_(\d+\.\d+\.\d+)_x64(?:-setup\.exe|\.msi)/.exec(artifact[0]);
    check(
      `README.md:${lineOf(readme, artifact.index)} ${artifact[0]}`,
      versionMatch?.[1],
    );
  }
}

const appVue = read("src/App.vue");
const appRe = /appState\?\.version \?\? '([^']+)'/;
const appMatch = appRe.exec(appVue);
if (appMatch) {
  check(`src/App.vue:${lineOf(appVue, appMatch.index)} version fallback`, appMatch[1]);
} else {
  errors.push({ label: "src/App.vue version fallback", message: "未找到" });
}

for (const label of oks) {
  console.log(`  OK   ${label}`);
}
for (const failure of failures) {
  console.log(`  FAIL ${failure.label} 期望 ${expected} 实际 ${failure.actual}`);
}
for (const error of errors) {
  console.log(`  ERROR ${error.label}：${error.message}`);
}

if (errors.length > 0) {
  console.error(
    `[version] 失败：${errors.length} 处无法校验，${failures.length} 处不一致（基准 ${expected}）`,
  );
  process.exitCode = 2;
} else if (failures.length > 0) {
  console.error(
    `[version] 失败：${failures.length} 处不一致（基准 ${expected}）。修复：npm run version:sync`,
  );
  process.exitCode = 1;
} else {
  console.log(`[version] 通过：${oks.length} 处载体一致（${expected}）`);
}
