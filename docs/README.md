# 项目资料索引

> 2026-10-16 整理。此处负责导航，当前事实从根上下文进入；带日期的计划、实验和验证记录保留当时范围。

## Agent 最短阅读路径

1. [AGENTS.md](../AGENTS.md)：仓库工作规则与发布合同。
2. [PROJECT.md](../PROJECT.md)：当前产品能力、范围、约束、启动方式与状态。
3. 按任务阅读 [ARCHITECTURE.md](../ARCHITECTURE.md)、[DESIGN.md](../DESIGN.md) 和 [Agent 迭代指南](agent-workflow.md)，再追到源码及测试。

## 开发与交付

| 资料 | 用途与阅读注意 |
| --- | --- |
| [开发记录](development.md) | 环境、命令、早期桌面操作与历次变更；部分早期文字已被后续章节和当前实现替代 |
| [使用许可](license.md) | 北京时间截止、NTP 与离线回退、前后端开始门禁 |
| [本地处理加固](processing-hardening.md) | 记录去参数、私有脚本、模板编码与保护边界 |
| [Windows x64 准备](windows-x64.md) | 平台准备、构建和安装检查；早期未验证状态需结合后续证据 |
| [Windows 安装模板](../src-tauri/windows/README.md) | 覆盖升级、修复、降级／运行中拒绝与模板来源 |
| [Windows 桌面验收](windows-desktop-acceptance.md) | 人工验收步骤及未完成项 |
| [macOS Intel 桌面验收](macos-intel-desktop-acceptance.md) | 安装后界面与媒体检查步骤 |
| [早期发行验证](release-verification.md) | 开发包安装与验证历史，不代表最新提交的发行状态 |
| [第三方材料](third-party-notices.md) | 打包材料现状与尚缺范围；不是正式发行完成证明 |
| [beta.8 发布说明](releases/0.0.1-beta.8.md) | 校验提醒后保存、处理模式记忆、Windows 路径显示及双平台验证限制 |
| [beta.7 发布说明](releases/0.0.1-beta.7.md) | 自定义变化、重置、输出目录记忆及双平台验证限制 |
| [beta.6 发布说明](releases/0.0.1-beta.6.md) | 上一版本的更新、下载命名、验证限制 |

## 实现来源与实验

| 资料 | 用途与边界 |
| --- | --- |
| [初版设计](superpowers/specs/2026-10-02-video-processing-desktop-design.md) / [初版计划](superpowers/plans/2026-10-02-video-processing-desktop.md) | 理解最初问题和实现路线；单任务时代的内容不能覆盖当前批次模型 |
| [文件夹导入与并发计划](plans/2026-10-06-1711-feat-folder-import-concurrent-processing-plan.md) | 需求、关键技术取舍、验收场景；是否落地以当前代码和测试为准 |
| [自定义变化设计](superpowers/specs/2026-10-16-custom-variation-design.md) / [实施计划](superpowers/plans/2026-10-16-custom-variation.md) | 已实现独立档位、克制推荐布局、可选裁切／变速与对应校验；校准取舍及平台未验范围查验证记录 |
| [需求及后续更新](requirements/20261002-research/requirement.md) | 目标与用户约束演变；需读后续对明显视听变化的授权 |
| [媒体分析报告](requirements/20261002-research/analysis-report.md) / [参考对分析](requirements/20261002-research/reference-pairs-analysis.md) | 样本技术差异；不构成通用平台审核效果证据 |
| [平台审核反馈矩阵](requirements/20261002-research/platform-review-matrix.md) | 已记录的成功／失败、样本与反馈范围 |
| [重复处理审计](requirements/20261002-research/repeat-processing-audit.md) | 固定参数重复输出的历史发现；当前变化预设已不同，不能直接沿用其现状结论 |
| [重复变化验收](requirements/20261002-research/repeat-variation-validation.md) / [组合变化验收](requirements/20261002-research/combined-variation-validation.md) | 旧预设的本地结果和用户反馈，保留失败证据 |
| [内容变化验收](requirements/20261002-research/content-variation-validation.md) | 当前默认方案的实现与批次反馈来源；不支持“任意输入每次必过” |

## 验证证据

优先打开匹配本次功能、版本和源码提交的记录，不按文件名日期推断“全部已验收”：

- [beta.6 本地验证](evidence/beta6-validation-20261012.json)：版本调整后的本地检查与限制。
- [beta.7 本地审查](evidence/beta7-local-review-20261017.json)：本轮增量的顺序自审、回归边界与未验范围；双平台构建证据随 Release 提供。
- [beta.8 本地审查](evidence/beta8-local-review-20261019.json)：校验提醒、模式记忆与 Windows 路径显示的顺序自审和原片回归范围；最终双平台证据随 Release 提供。
- [处理加固验证](evidence/processing-hardening-20261012.json)：绑定提交的本地、Windows、macOS 原生范围。
- [Windows 安装升级验证](evidence/windows-installer-upgrade-20261012.json)：升级、修复、降级及运行中拒绝场景。
- 其他 `docs/evidence/` JSON 保存早期平台和版本的证据；测试计数、资源校验、人工桌面验收、签名／公证与正式发行状态须分别解读。

## 资料归档规则

根上下文是持续更新的当前说明；`requirements/` 保存需求与实验；`plans/`、`superpowers/` 保存规划历史；`evidence/` 保存具体提交与平台的验证；`releases/` 保存版本说明。未来 `solutions/` 只存符合 ce-compound 门槛的持久经验。

`references/` 是参考素材，不自动成为已采纳设计规范；新素材是否纳入版本控制按任务范围判断。测试视频和临时产物留在忽略路径，不能随文档归档进仓库或发布附件。

- [自定义变化验证记录](evidence/custom-variation-20261016.json)：本轮测试、媒体、浏览器与原生资源检查范围；不等于双平台发行验收。
