# 贡献指南

## 环境

- Windows 10/11，已安装 WebView2 Runtime
- Node.js >= 22.12（仓库提供 `.nvmrc`）
- Rust 1.99.1（仓库提供 `rust-toolchain.toml`）及 MSVC 工具链
- 建议在 Developer Command Prompt 中构建，确保 `rc.exe` 可用

## 开发

```powershell
npm ci
npm run tauri dev
```

只读诊断：

```powershell
cargo run --manifest-path src-tauri/Cargo.toml -- --selftest
cargo run --manifest-path src-tauri/Cargo.toml -- --scan-json
```

## 提交前检查

```powershell
npm run check
```

该命令会执行版本一致性检查、前端类型检查与构建、`cargo fmt --check`、`cargo clippy -D warnings` 和 Rust 测试。

`cargo test` 会在 `HKCU\Software\StartupManagerTest*` 下创建并清理临时键。涉及真实启动项、计划任务或 UWP 的写操作验收必须在虚拟机快照中执行，见 `docs/ACCEPTANCE.md`。

## 代码约定

- 界面文案和错误信息使用简体中文。
- 提交信息使用 conventional commit，例如 `fix: ...`、`feat: ...`、`chore: ...`。
- 不要提交 `release/`、`dist/`、`src-tauri/target/`、`node_modules/` 或包含个人路径/密钥的文件。
- 修改权限、提权 helper、注册表写入或快照格式时，请同时更新 README、`docs/ACCEPTANCE.md` 和相关测试。

## 版本与发布

版本以 `package.json` 为准：

```powershell
npm run version:sync -- 0.2.0
npm run version:check
```

发布构建：

```powershell
powershell -ExecutionPolicy Bypass -File tools/release.ps1
```

发布产物为 NSIS、MSI、免安装 exe 和 `SHA256SUMS.txt`。推送 `vX.Y.Z` tag 会触发 GitHub Release 工作流。

## 许可证

本项目使用 MIT 许可证，详见 [LICENSE](LICENSE)。提交贡献即表示同意在 MIT 许可证下发布。
