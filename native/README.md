# VectorCraft 科研定制版（第二版开发预览）

基于官方 VectorCraft v0.10.0 的独立 Windows 桌面构建，在“属性 / 图层 / 库”旁增加“科研组图”原生停靠面板。科研操作在同一进程内通过 VectorCraft 命令执行，不需要首版 Python 工具箱或后台网页服务。

当前候选版本为 **0.2.0-rc.1**，已迁移首版的主要功能：

- 按列数、行列净间距排图；可先统一每个对象的宽高并保持比例。
- 六向对齐、整体边界或第一个对象参考、水平/垂直净间距分布。
- A/a/A)/a)/数字标签；字体、字号、六位十六进制颜色、四角锚点和偏移；按对象关联重复更新。
- 绝对/相对位置复制粘贴、两个对象交换位置。
- 添加或更新独立可编辑的矩形边框。
- 最多 32 个命名预设，保存、载入、删除；重启后恢复。
- 每项编辑一次撤销；失败取消事务。
- 面板支持原有停靠、浮动和工作区保存，参数随定制版设置保存。

候选版完整上游 CI 的 7 个阶段全部通过，最终发行二进制另通过 68 项真实桌面回归，见 [RC1 验证记录](../docs/native/VALIDATION-RC1.md)。正式发布仍需以发行记录为准。第一阶段的最小内嵌样例 `0.2.0-preview.1` 独立保留，见 [样例验证记录](../docs/native/VALIDATION.md)。

## 使用

运行独立的 `VectorCraftSCI.exe`，在右侧“属性 / 图层 / 库”旁点击“科研组图”，或从“窗口”菜单打开。选择原图、文字或组合后展开所需分区。面板较长时可向下滚动，也可收起暂时不用的分区。

尺寸、间距、偏移均为 pt。宽/高留空表示不改变；保持比例时只填其中一项。标签颜色和边框颜色填写 `#RRGGBB`。标签和边框可重复更新，但不会自动跟随移动的原图。

复制相对位置时，第一个排序对象为参考；粘贴保持目标参考对象原位，其余对象恢复相对偏移。位置剪贴板仅存在于本次运行，重启清空。接口选择顺序不保证鼠标点击顺序，需要确定顺序时请选择按行、水平或垂直排序。

旋转、组合和裁剪对象以轴向几何边界排布；不同时选择父组合和其内部对象，不处理隐藏、锁定、空对象或零宽/零高对象。单次最多 200 个对象。

## 原生调用

面板与本机控制接口共用 `ui.scientific` 命令。`operation: inspect` 返回当前参数、预设名和剪贴板状态；`settings` 是对当前参数的合并更新。

```json
{"command":"ui.scientific","params":{"operation":"grid","settings":{"columns":2,"gap_x":18,"gap_y":24,"width":"130","height":"","keep_ratio":true}}}
```

其余 operation 为 `resize`、`align`、`spacing`、`labels`、`borders`、`copy_position`、`paste_position`、`swap`、`preset_save`、`preset_load`、`preset_delete`；对齐/分布方向由 `axis` 给出。执行这些命令不需要 AI 模型或 API Key。

## 源码组织

`upstream.json` 固定官方源码身份；`patches/host-integration.patch` 接入停靠系统、命令入口和独立设置；`overlay/` 保存独立科研模块。构建脚本在英文路径下取得上游并应用这些变更，不在本仓复制整个上游历史。

官方 WASM ABI v1 支持路径筛选器和实时几何效果，但明确不包含面板插件，也不处理文字和图片。因此本版使用原生源码定制：[官方插件文档](https://github.com/storytold/vectorcraft/blob/v0.10.0/docs/plugins.md)。

## 构建

需要 Git、Rust 1.95 或更高版本以及 Windows C++ 链接工具。可用 MSVC 工具链，或 Rust Windows GNU 目标加完整 MinGW-w64。所有源码、工具缓存和输出路径均应使用 ASCII 字符；部分 GNU 工具不能处理中文路径。

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build_native_windows.ps1 -RuntimeRoot C:\VectorCraftSciBuild
```

构建脚本不会安装系统组件；先准备工具链。当前定制版默认设置目录为 `%APPDATA%\VectorCraftSCI`，不迁移官方版设置；`VECTORCRAFT_CONFIG_DIR` 仍可显式覆盖。

上游代码及其原有资源遵循上游许可与声明，本项目原始科研模块遵循根目录 MIT 许可。发行包必须包含上游 LICENSE-MIT、LICENSE-APACHE、NOTICE 和所用资源的许可。定制版未由上游官方发布或签名。
