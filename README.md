# 启动项管理器

Windows 平台的可逆启动项管理工具。Rust 后端直接调用 Windows API，界面使用 Tauri 2.12 + Vue 3.5 + Tailwind CSS 4 + shadcn-vue。

界面为全中文，默认跟随系统深浅色，支持搜索、筛选、详情、签名/发布者显示和快照差异对比。

## 功能

- 扫描并展示以下启动项：
  - HKCU / HKLM 的 `Run`、`RunOnce`（HKLM 区分 64/32 位视图；HKCU 的 Run 键由 32/64 位应用共享，只扫描一次）
  - 当前用户 / 公共启动文件夹（解析 `.lnk` 目标）
  - UWP / MSIX `StartupTask`
  - 计划任务中 boot / logon 触发的任务
  - 自动启动服务（只读）
- 可逆启用/禁用：
  - 注册表与启动文件夹：写 `StartupApproved` 的 12 字节状态值，不删除原启动项
  - 优先沿用 Explorer 的当前用户覆盖：HKLM / 公共启动文件夹项若存在 HKCU 的 `StartupApproved` 值，状态按该值显示并写回 HKCU
  - UWP：写 `State` DWORD
  - 计划任务：`IRegisteredTask.Enabled`
- 显示发布者与 Authenticode 签名状态（catalog 签名会标为“签名未验证”，不会误报为未签名；签名被篡改、证书过期或不受信会标为“签名无效”）
- 列表和详情显示每个启动项可执行文件自身的图标；快捷方式解析目标图标，UWP 或无 exe 的项回退来源图标
- 搜索、筛选、来源/作用域/位数标识、详情抽屉、打开文件位置、复制命令行
- 列表中的警示图标支持悬浮或键盘聚焦，逐条查看警告原因
- 快照保存、差异对比（新增 / 移除 / 变化项）与删除（带二次确认）；任一来源读取失败时会拒绝保存或对比，避免生成不完整快照
- 深浅主题、跟随系统、快捷键 `Ctrl+F`、`F5`、`Esc`

## 不做什么

- 不新建、不删除启动项；禁用始终可逆。
- 不修改服务启动类型（服务页只读，避免误改系统关键服务）。
- 不扫描其他用户的 hive、不提供多用户切换。
- 不做托盘常驻、自动更新、延迟启动、“启动影响”评分（无公开 API）。
- 不提供 Winlogon / IFEO / AppInit_DLLs 等深层 ASEP 的写操作。这些位置风险过高，后续如需只做只读诊断。

## 权限模型

主进程以 `asInvoker` 启动，用户级项（HKCU、用户启动文件夹、UWP）全程不需要 UAC。

系统级项（HKLM、公共启动文件夹、以其他用户或组为主体运行的计划任务）通过同一个 exe 的 `--elevated-helper` 子命令按需提权；以当前用户为主体的计划任务按用户级处理，不弹 UAC：

1. 主进程创建一次性本地套接字（随机名称 + nonce）。
2. `ShellExecuteW("runas", ...)` 拉起提权 helper。
3. helper 重新扫描启动项，按 ID 重新定位并校验作用域，同时核对父进程预期的目标动作，只执行白名单动作，写后读回校验。
4. helper 使用不解析文件元数据的轻量扫描，避免以管理员身份访问 UNC/设备路径；同时校验 IPC 对端镜像与返回的 ID。
5. helper 不加载 WebView；不会暴露“任意注册表写入”能力。

存在当前用户 `StartupApproved` 覆盖的 HKLM / 公共启动文件夹项会写 HKCU，不需要 UAC；其余系统级项才走 helper。

这是安全边界：即使前端被注入，也只能对已枚举的启动项做启用/禁用，不能构造任意路径写入。

## 开发

环境要求：

- Windows 10/11，已安装 WebView2 Runtime（Win11 自带）
- Node.js >= 22.12
- Rust 1.88+（edition 2024）及 MSVC 工具链（`rc.exe` 需在 PATH 中，建议使用 Developer Command Prompt）

```powershell
npm ci
npm run tauri dev
```

如果机器上已经有一个正在运行的实例，需要同时调试第二个窗口，可用（仅调试构建支持）：

```powershell
cargo run --manifest-path src-tauri/Cargo.toml -- --allow-multiple
```

前端类型检查与构建：

```powershell
npm run build
```

版本号以 `package.json` 为准；发布前同步（脚本会自动校验）：

```powershell
npm run version:sync -- 0.2.0
npm run version:check
```

Rust 测试：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

只读诊断：

```powershell
cargo run --manifest-path src-tauri/Cargo.toml -- --selftest
cargo run --manifest-path src-tauri/Cargo.toml -- --scan-json
powershell -ExecutionPolicy Bypass -File tools/verify.ps1
```

图标源文件为 `assets/icon.svg`；修改后运行 `npx tauri icon assets/icon.svg` 重新生成 `src-tauri/icons/` 下的 Windows 图标。

打包（NSIS + MSI，perMachine）：

```powershell
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/nsis/` 和 `src-tauri/target/release/bundle/msi/`。NSIS 安装包按 `productName` 生成，实际文件名是 `启动项管理器_0.1.0_x64-setup.exe`；MSI 实际文件名是 `启动项管理器_0.1.0_x64_zh-CN.msi`。复制到 `release/` 时重命名为下面的英文名，并重新生成 `SHA256SUMS.txt`。也可以直接运行 `powershell -ExecutionPolicy Bypass -File tools/release.ps1` 自动完成版本校验、构建、复制、重命名和哈希生成。

- `StartupManager_0.1.0_x64-setup.exe`（NSIS 安装包，perMachine）
- `StartupManager_0.1.0_x64.msi`（MSI 安装包，WiX）
- `StartupManager.exe`（免安装可执行文件，需要系统 WebView2）
- `SHA256SUMS.txt`

校验下载产物：

```powershell
Get-FileHash .\release\StartupManager.exe -Algorithm SHA256
Get-Content .\release\SHA256SUMS.txt
```

或在 Git Bash 中：

```bash
cd release && sha256sum -c SHA256SUMS.txt
```

MSI 为 per-machine 安装，安装/卸载需要管理员授权：

```powershell
msiexec /i StartupManager_0.1.0_x64.msi
msiexec /x StartupManager_0.1.0_x64.msi
```

`release/`、`dist/`、`.ui-dump/` 都是本地构建/验收产物，已在 `.gitignore` 中忽略，不随仓库提交。`dist/` 是前端构建输出，不是发布目录。

## 数据目录

`%LOCALAPPDATA%\StartupManager\`

- `logs/startup-manager.log.*`：按天滚动的日志，记录扫描来源、数量、错误和启停操作（不记录完整命令行）
- `snapshots/snapshot-*.json`：快照；包含启动项命令行，可能含敏感参数，分享前请自行检查
- `actions.jsonl`：启停操作审计记录

## 设计约束与已知限制

- `StartupApproved` 是自 Windows 8 起的事实标准，但**没有公开文档**。实现只读取首字节的最低位判断启用/禁用，并保留原状态家族（0x02/0x03、0x06/0x07、0x0A/0x0B 等）；缺失值视为启用，长度不是 12 字节时标为状态未知；写入后立即严格读回校验。
- 修改注册表/启动文件夹的启动项通常在**下次登录**生效，界面不会承诺“立即停止运行”。计划任务的修改通常立即生效。
- `RunOnce` 只执行一次，界面只读展示，不提供开关。
- 计划任务只列出 boot/logon 触发器，避免把几百条更新检查任务混进列表；`\Microsoft\` 路径和 SYSTEM 账户任务标记只读。
- 服务列表只读，显示自动启动的 Win32 服务；驱动与内核服务不在列表中。
- UWP 启动任务只覆盖当前用户，显示名暂时使用包族名/TaskId。
- 签名验证只验证嵌入的 Authenticode 签名；系统文件常用的 catalog 签名会显示“签名未验证”；摘要不匹配、证书过期或链不受信会显示“签名无效”；UNC/设备路径跳过签名与版本查询。
- 快照文件名带纳秒时间戳，写入采用临时文件 + 改名；列表按解析后的时间排序。
- 启动项数量通常不大，表格未做虚拟滚动；服务较多时建议用搜索/筛选。
- 未做代码签名，首次运行可能出现 SmartScreen 提示；发布前建议使用 OV/EV 证书签名。

## 排错

- **点击系统级开关后没有变化**：确认 UAC 弹窗是否被取消；取消会提示“已取消管理员授权”。
- **计划任务读取失败**：普通权限下部分系统任务不可读，界面会保留其他来源并显示错误横幅。
- **系统级项无法禁用**：Microsoft 系统任务、SYSTEM 任务、RunOnce、服务为只读；其余系统级项通常需要管理员授权（存在当前用户覆盖时除外）。
- **杀软误报**：工具会枚举启动项、写 StartupApproved 并请求提权，可能触发启发式。发布时请提供 SHA-256 与源码，不要使用“批量删除/一键清理”式功能。
- **主题切换后标题栏颜色不变**：应用会请求同步原生标题栏主题；若系统策略覆盖或窗口管理器忽略请求，标题栏可能仍跟随系统。应用内主题已经切换。

## 验收

写操作验收请使用虚拟机快照，不要在主力机上直接操作真实启动项。完整矩阵见 [docs/ACCEPTANCE.md](docs/ACCEPTANCE.md)。

## 许可证

MIT，详见 [LICENSE](LICENSE)。
