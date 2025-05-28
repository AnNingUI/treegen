<p align="center">
<a href="./README_en.md"> English </a> | <span> 简体中文 </span>
</p>

# 项目：treegen

treegen 是一个用于根据 Markdown、YAML、JSON、TOML 和 JSON5 描述生成文件/目录树的工具。

## 为什么要写这个工具？
再面向AI编程时，经常需要让AI生成一个项目目录结构，而根据项目目录结构手动创建文件与文件夹十分麻烦。因此，开发了这个工具，可以根据 Markdown、YAML、JSON、TOML 和 JSON5 描述生成文件/目录树。

## 功能
- 支持多种文档格式
- 预览生成的文件和目录结构
- 支持自定义权限和清理已有目录

## 使用方法
1. 准备好描述文件（例如：tree.md、tree.yaml 等）。
2. 运行命令行工具指定输入文件和输出目录。
3. 查看输出结果，默认会在指定输出目录下生成对应的文件树。

## 示例命令
```
treegen tree.yaml --out output --verbose
```

## 树格式
例如：
project/
├── src/
│   ├── main.rs
│   └── lib.rs
├── Cargo.toml
└── README.md

更多格式请查看example

## 命令参数说明
- 要解析的一个或多个输入文件（支持 .md、.yaml、.yml、.json、.toml、.json5）。
- out: 输出根目录（可选，默认是当前工作目录）。
- dry_run: 仅预览将要创建的文件/目录，不写入磁盘。
- verbose: 打印详细日志，显示每个文件/目录创建情况。
- clean: 如果输出目录已存在同名路径，先删除再创建（谨慎使用）。
- mode: 新建文件的权限（八进制，如 0o644，仅类 Unix 平台生效）。