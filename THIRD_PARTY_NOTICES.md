# 来源与第三方说明

本项目为独立实现的 VectorCraft 配套工具，不属于 VectorCraft 或 SCI Toolbox 官方项目。

- 功能需求参考：Achuan-2 的 [SCI Toolbox](https://github.com/Achuan-2/illustrator_sci_toolbox)，原项目采用 AGPL-3.0。本项目未复制其程序源码、界面资源、图标或发行包。独立编写布局、标签管理和连接程序，没有将原项目作为运行依赖。
- VectorCraft 采用其[官方控制协议](https://github.com/storytold/vectorcraft/blob/main/docs/control-protocol.md)。用户自行安装 VectorCraft；本项目不分发其主程序、字体或素材。
- Python 源码运行时仅使用标准库。Windows 打包使用 PyInstaller；其许可和相关依赖见 PyInstaller 官方说明。打包产物中的第三方运行组件保留在压缩包的 notices 目录中。
- 示例图像由测试程序合成，用于功能验证，不代表实验数据或科学结果。

未来如引入第三方代码或素材，须逐项记录其来源和许可，不能沿用上述“未复制”的声明而不作更新。
# 原生科研定制版

`native/` 中的接入补丁用于修改 [storytold/vectorcraft v0.10.0](https://github.com/storytold/vectorcraft/tree/v0.10.0)，固定提交为 `71174477cc056020fe712ae59901d76b870ca9d9`。上游代码/原有资产遵循其 MIT OR Apache-2.0 许可、NOTICE、ASSETS.md 和单独资产许可；本项目原始科研模块遵循根目录 MIT 许可。定制程序不是上游官方发行。

Windows 本机构建使用 Rust 与 MinGW-w64/GCC。它们是构建工具，不随应用分发；分发程序时需保留上游及实际链接运行组件适用的许可/声明。可选 craft-fonts 未作为本次构建输入；未捆绑用户安装的系统字体。
