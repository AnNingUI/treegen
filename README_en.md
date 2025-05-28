> This English Readme is translated by `o3-mini`, please understand if there is any incomprehension.

---

<p align="center">
<span> English </span> | <a  href="./README.md"> 简体中文 </a>
</p>

# Project: treegen

treegen is a tool that generates file/folder trees based on specifications written in Markdown, YAML, JSON, TOML, and JSON5.

## Why should you write this tool?
When programming for AI, it is often necessary to have AI generate a project directory structure, and it is very troublesome to manually create files and folders based on the project directory structure. Therefore, this tool was developed to generate a file/directory tree based on Markdown, YAML, JSON, TOML, and JSON5 descriptions.

## Features
- Supports multiple document formats
- Preview the structure before creating files/directories
- Allows custom file permissions and cleaning pre-existing directories

## Usage
1. Prepare a specification file (e.g., tree.md, tree.yaml, etc.).
2. Run the command-line tool with the specification file and output directory options.
3. Check the output where the generated file tree will be created.

## Example Command
```
treegen tree.yaml --out output --verbose
```

## Tree Format
Example:
project/
├── src/
│   ├── main.rs
│   └── lib.rs
├── Cargo.toml
└── README.md

## Command Parameters
- specification: One or more specification files (supports .md, .yaml, .yml, .json, .toml, .json5).
- out: Output root directory; default is the current working directory.
- dry_run: Preview actions without writing to disk.
- verbose: Print detailed logs for every file/directory creation.
- clean: Clean existing same-named paths in the output directory before creation.
- mode: Permission for created files (octal, e.g., 0o644), effective on Unix platforms.
