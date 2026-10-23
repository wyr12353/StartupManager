# StartupManager (启动项管理器)

Windows 平台轻量、可逆的现代化启动项管理工具。

[![Release](https://img.shields.io/github/v/release/wyr12353/StartupManager?color=blue&logo=github)](https://github.com/wyr12353/StartupManager/releases)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%7C%2011-0078D6?logo=windows)](https://github.com/wyr12353/StartupManager)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB?logo=tauri)](https://tauri.app/)
[![Vue](https://img.shields.io/badge/Vue-3.5-4FC08D?logo=vuedotjs)](https://vuejs.org/)
[![Rust](https://img.shields.io/badge/Rust-2024-DEA584?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

---

## ✨ 核心特性

- **全面扫描覆盖**：支持扫描注册表（HKCU/HKLM `Run`、`RunOnce`）、启动文件夹、UWP `StartupTask`、计划任务与系统服务。
- **安全可逆禁用**：原生采用 Windows 标准启动控制机制（`StartupApproved`），禁用仅改变启动状态，不破坏原有键值与快捷方式。
- **最小权限模型**：日常以普通用户权限运行；仅在修改系统级启动项时按需安全提权（UAC），兼顾便捷与安全性。
- **状态快照对比**：支持一键导出当前启动项快照，轻松对比不同系统状态下的新增、移除与变更项。
- **现代交互体验**：界面全中文设计，原生跟随系统浅色/深色主题，提供实时搜索、签名验证与快捷键支持。

---

## 🚀 下载使用

前往 [Releases](https://github.com/wyr12353/StartupManager/releases) 页面下载适合您的版本：

| 文件 | 说明 |
| :--- | :--- |
| **`StartupManager.exe`** | 便携免安装版，双击直接运行 |
| **`StartupManager_0.1.1_x64-setup.exe`** | 常用安装向导包（自动创建快捷方式） |
| **`StartupManager_0.1.1_x64.msi`** | 企业/静默分发安装包 |

> 💡 **系统要求**：Windows 10 / Windows 11（需要系统内置 WebView2 运行时，Win11 已默认内置）。

---

## 🛠️ 本地开发

### 环境要求
- **操作系统**：Windows 10 / 11
- **Node.js**：>= 22.12
- **Rust**：1.88+（Edition 2024）与 MSVC 构建工具链

### 运行与调试
```powershell
# 1. 安装前端依赖
npm install

# 2. 启动开发调试模式
npm run tauri dev
```

### 生产打包
```powershell
npm run tauri build
```
打包产物将输出在 `src-tauri/target/release/bundle/` 目录下。

---

## 📁 本地数据说明

应用数据默认保存在 `%LOCALAPPDATA%\StartupManager\` 目录下：
- `snapshots/`：用户保存的启动项快照数据
- `logs/`：按天滚动的应用运行日志
- `actions.jsonl`：操作审计记录

---

## 📄 开源协议

本项目基于 [MIT License](LICENSE) 开源。
