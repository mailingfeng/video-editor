## 需求背景
有一个Windows视频处理软件，可以将 某个原视频 （docs/requirements/20261002-research/原视频.mp4） 编辑处理后变成 新视频（docs/requirements/20261002-research/编辑后视频.mp4），但其视频内容完全一样，但视频平台只会识别为新视频，而不是识别为重复/搬运/抄袭检测；

## 需求目标
1. 分析 原视频 和 编辑后视频， 分析该视频处理软件的实现原理，生成分析报告(markdown)
  > 目前已知有使用 ffmpeg.exe 处理工具；

2. 给出实现类似功能，适合 windows 和 mac 的桌面软件落地方案；