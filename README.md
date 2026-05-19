# Q JK 桌宠

Rust 原生桌面宠物应用，已移除 Electron 和 Node 依赖。

## 启动

```bash
./run.sh
```

首次启动会自动执行 release 编译，需要本机已安装 Rust/Cargo。

## 功能

- 原生透明桌宠窗口
- 原生系统托盘菜单
- 动作选择、随机动作、大小和速度调节
- 鼠标拖动、靠近、离开、点击响应
- 键盘输入和滚轮活动监听
- macOS 开机自启和辅助功能权限入口

## 资源

运行时动作帧位于：

```text
assets/frames/<action-id>/<frame>.png
```

托盘和应用图标位于：

```text
assets/icons/
```

## GitHub Actions

仓库包含跨平台构建 workflow：

```text
.github/workflows/build.yml
```

推送到 `main` 后会构建：

- macOS `.app`
- Windows 可执行包
- Linux 可执行包

构建产物可在 GitHub Actions 的 artifacts 中下载。
