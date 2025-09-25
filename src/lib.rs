use anyhow::{bail, Context, Result};
use clap::Parser;
use regex::Regex;
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// 添加 rayon 导入
use rayon::prelude::*;

/// CLI 参数定义
#[derive(Parser, Debug)]
#[command(name = "treegen")]
#[command(author = "AnNingUI <3533581512@qq.com>")]
#[command(version = "0.1.0")]
#[command(
    about = "Generate file/folder trees from Markdown/YAML/JSON/TOML/JSON5 specifications",
    long_about = None
)]
pub struct Args {
    /// 要解析的一个或多个输入文件（支持 .md/.yaml/.yml/.json/.toml/.json5）
    #[arg(required = true)]
    pub input: Vec<PathBuf>,

    /// 输出根目录（可选，默认是当前工作目录）
    #[arg(short, long)]
    pub out: Option<PathBuf>,

    /// 仅预览将要创建的文件/目录，不写入磁盘
    #[arg(long)]
    pub dry_run: bool,

    /// 打印详细日志（每个文件/目录创建情况）
    #[arg(short, long)]
    pub verbose: bool,

    /// 覆盖已存在的文件
    #[arg(long)]
    pub force: bool,

    /// 如果输出目录已存在同名路径，先删除再创建（谨慎使用）
    #[arg(long)]
    pub clean: bool,

    /// 新建文件的权限（八进制，如 0o644，仅类 Unix 平台生效）
    #[arg(long, default_value = "0o644")]
    pub mode: String,

    /// 并行工作线程数（0 = 自动选择）
    #[arg(short = 'j', long, default_value = "0")]
    pub jobs: usize,

    /// 批量日志输出：每 N 个文件输出一次进度（0 = 禁用批量日志）
    #[arg(long, default_value = "100")]
    pub batch_log: usize,
}

#[derive(Debug, Clone)]
pub struct CreateFsOptions {
    pub dry_run: bool,
    pub verbose: bool,
    pub force: bool,
    pub mode: u32,
    pub jobs: usize,
    pub batch_log: usize,
}

/// 节点类型：目录或文件
#[derive(Debug, Clone)]
pub enum NodeType {
    Dir,
    File,
}

// parse_file trait 用于解析输入文件，返回 Node 树
pub trait ParseFile {
    fn parse_file(path: &Path) -> Result<Node>;
}

/// 树节点结构
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    node_type: NodeType,
    pub children: Vec<Node>,
    pub content: Option<String>, // 文件内容
}

struct MdParser;
struct YamlParser;
struct JsonParser;
struct Json5Parser;
struct TomlParser;

impl Node {
    pub fn new_file(name: String, content: Option<String>) -> Self {
        Node {
            name,
            node_type: NodeType::File,
            children: Vec::new(),
            content,
        }
    }
    pub fn new_dir(name: String) -> Self {
        Node {
            name,
            node_type: NodeType::Dir,
            children: Vec::new(),
            content: None,
        }
    }
}

/// SerdeNode 用于反序列化
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SerdeNode {
    Str(String),
    Map(BTreeMap<String, SerdeNode>),
}

/// 去除多行字符串的首尾空行 + 公共缩进
fn dedent(s: &str) -> String {
    let mut lines: Vec<&str> = s.lines().collect();
    // 去掉首尾纯空行
    while !lines.is_empty() && lines.first().unwrap().trim().is_empty() {
        lines.remove(0);
    }
    while !lines.is_empty() && lines.last().unwrap().trim().is_empty() {
        lines.pop();
    }
    if lines.is_empty() {
        return String::new();
    }
    // 计算最小缩进
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|&c| c == ' ').count())
        .min()
        .unwrap_or(0);
    // 去除缩进
    lines
        .into_iter()
        .map(|l| {
            if l.len() >= min_indent {
                l[min_indent..].to_string()
            } else {
                l.trim_start().to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// 预编译正则
static TREE_REGEX: OnceLock<Regex> = OnceLock::new();

fn tree_regex() -> &'static Regex {
    TREE_REGEX.get_or_init(|| {
        Regex::new(r"^(?P<indent>(│   |    )*)(?P<prefix>├── |└── )?(?P<name>.+)$")
            .expect("Failed to compile tree regex")
    })
}

/// 并行创建文件系统
pub fn create_fs_parallel(base: &Path, root: &Node, opts: &CreateFsOptions) -> Result<()> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(if opts.jobs == 0 {
            num_cpus::get()
        } else {
            opts.jobs
        })
        .build()
        .context("Failed to create thread pool")?;

    create_dirs_sequential(base, root, opts.dry_run, opts.verbose, opts.batch_log)?;

    pool.install(|| {
        create_files_parallel(
            base,
            root,
            opts.dry_run,
            opts.verbose,
            opts.force,
            opts.mode,
            opts.batch_log,
        )
    })
}

/// 顺序创建目录结构
fn create_dirs_sequential(
    base: &Path,
    root: &Node,
    dry_run: bool,
    verbose: bool,
    batch_log: usize,
) -> Result<()> {
    let mut stack = vec![(base.to_path_buf(), root)];
    let mut dir_count = 0;

    while let Some((parent, node)) = stack.pop() {
        let path = if node.name.is_empty() {
            parent
        } else {
            parent.join(&node.name)
        };

        if let NodeType::Dir = node.node_type {
            dir_count += 1;

            // 批量日志输出
            if verbose && (batch_log == 0 || dir_count % batch_log == 0) {
                eprintln!(
                    "{}Creating directory {}/...",
                    if dry_run { "[Dry-Run] " } else { "" },
                    dir_count
                );
            } else if verbose && batch_log == 1 {
                eprintln!(
                    "{}Create directory: {}",
                    if dry_run { "[Dry-Run] " } else { "" },
                    path.display()
                );
            }

            if !dry_run {
                fs::create_dir_all(&path)
                    .with_context(|| format!("Failed to create directory '{}'", path.display()))?;
            }
            // 子目录倒序入栈，保持原顺序
            for child in node.children.iter().rev() {
                stack.push((path.clone(), child));
            }
        }
    }

    if verbose && batch_log > 1 {
        eprintln!("✅ Created {dir_count} directories");
    }

    Ok(())
}

/// 并行创建文件
fn create_files_parallel(
    base: &Path,
    root: &Node,
    dry_run: bool,
    verbose: bool,
    force: bool,
    _mode: u32,
    batch_log: usize,
) -> Result<()> {
    // 收集所有需要创建的文件
    let files = collect_files(base, root);
    let total_files = files.len();

    if dry_run {
        if verbose {
            if batch_log > 1 {
                eprintln!("[Dry-Run] Would create {total_files} files");
            } else if batch_log == 1 {
                for (path, _) in &files {
                    eprintln!("[Dry-Run] Create file: {}", path.display());
                }
            }
        }
        return Ok(());
    }

    // 并行处理文件创建
    files
        .par_iter()
        .enumerate()
        .try_for_each(|(index, (path, content)): (usize, &(PathBuf, Option<String>))| -> Result<(), anyhow::Error> {
            // 批量日志输出
            if verbose && batch_log > 0 && index % batch_log == 0 {
                eprintln!("Creating file {}/{}...", index + 1, total_files);
            } else if verbose && batch_log == 0 {
                eprintln!("Create file: {}", path.display());
            }

            if let Some(parent) = path.parent() {
                // 目录应该已经存在，但以防万一
                fs::create_dir_all(parent).ok();
            }

            let content_str = content.as_deref().unwrap_or("");

            // 如果文件已存在且未启用 --force，则报错
            if path.exists() && !force {
                bail!("File '{}' already exists (use --force to overwrite)", path.display());
            }

            fs::write(path, content_str)
                .with_context(|| format!("Failed to write file '{}'", path.display()))?;

            #[cfg(unix)]
            fs::set_permissions(path, fs::Permissions::from_mode(_mode))
                .with_context(|| format!("Failed to set permissions for '{}'", path.display()))?;

            Ok(())
        })?;
    if verbose && batch_log > 1 {
        eprintln!("✅ Created {total_files} files");
    }

    Ok(())
}

/// 收集所有文件路径和内容
fn collect_files(base: &Path, node: &Node) -> Vec<(PathBuf, Option<String>)> {
    let mut files = Vec::new();
    let mut stack = vec![(base.to_path_buf(), node)];

    while let Some((parent, node)) = stack.pop() {
        let path = if node.name.is_empty() {
            parent
        } else {
            parent.join(&node.name)
        };

        match node.node_type {
            NodeType::File => {
                files.push((path, node.content.clone()));
            }
            NodeType::Dir => {
                for child in node.children.iter().rev() {
                    stack.push((path.clone(), child));
                }
            }
        }
    }
    files
}

impl SerdeNode {
    fn to_node(&self, name: String) -> Node {
        match self {
            SerdeNode::Str(content) => {
                let dedented = dedent(content);
                Node::new_file(name, Some(dedented))
            }
            SerdeNode::Map(map) => {
                let mut dir = Node::new_dir(name);
                for (k, v) in map {
                    dir.children.push(v.to_node(k.clone()));
                }
                dir
            }
        }
    }
}

impl MdParser {
    fn parse_tree(lines: &[&str]) -> Result<Node> {
        let mut root = Node::new_dir("".to_string());
        // 栈存 (level, 父节点在 children 中的索引路径)
        let mut stack: Vec<(usize, Vec<usize>)> = vec![(0, Vec::new())];

        let re = tree_regex();

        for line in lines {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            let caps = re
                .captures(line)
                .with_context(|| format!("Line '{line}' does not match Markdown tree format"))?;

            let indent_str = caps.name("indent").map_or("", |m| m.as_str());
            let indent_blocks = indent_str.chars().count() / 4;
            let level = if caps.name("prefix").is_some() {
                indent_blocks + 2
            } else {
                indent_blocks + 1
            };

            let name: Cow<str> = {
                let n = caps.name("name").unwrap().as_str().trim();
                if n.contains(':') {
                    Cow::Owned(n.replace(':', "_"))
                } else {
                    Cow::Borrowed(n)
                }
            };

            let node_type = if name.ends_with('/') {
                NodeType::Dir
            } else {
                NodeType::File
            };

            // 弹出直到找到父节点
            while let Some(&(last_level, _)) = stack.last() {
                if last_level >= level {
                    stack.pop();
                } else {
                    break;
                }
            }

            // 获取父节点可变引用
            let parent = {
                let (_, ref indices) = stack.last().unwrap();
                let mut current: &mut Node = &mut root;
                for &idx in indices {
                    current = &mut current.children[idx];
                }
                current
            };

            // 添加新节点
            let new_node = if let NodeType::Dir = node_type {
                Node::new_dir(name.into_owned())
            } else {
                Node::new_file(name.into_owned(), None)
            };
            parent.children.push(new_node);

            // 如果是目录，压入栈
            if let NodeType::Dir = node_type {
                let mut new_indices = stack.last().unwrap().1.clone();
                new_indices.push(parent.children.len() - 1);
                stack.push((level, new_indices));
            }
        }

        Ok(root)
    }
}

impl ParseFile for MdParser {
    fn parse_file(path: &Path) -> Result<Node> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read Markdown file '{}'", path.display()))?;
        let lines: Vec<&str> = content.lines().collect();
        MdParser::parse_tree(&lines)
    }
}

impl ParseFile for YamlParser {
    fn parse_file(path: &Path) -> Result<Node> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read YAML file '{}'", path.display()))?;
        let data: BTreeMap<String, SerdeNode> = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse YAML in '{}'", path.display()))?;
        let mut root = Node::new_dir("".to_string());
        for (k, v) in data {
            root.children.push(v.to_node(k));
        }
        Ok(root)
    }
}

impl ParseFile for JsonParser {
    fn parse_file(path: &Path) -> Result<Node> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read JSON file '{}'", path.display()))?;
        let data: BTreeMap<String, SerdeNode> = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse JSON in '{}'", path.display()))?;
        let mut root = Node::new_dir("".to_string());
        for (k, v) in data {
            root.children.push(v.to_node(k));
        }
        Ok(root)
    }
}

impl ParseFile for Json5Parser {
    fn parse_file(path: &Path) -> Result<Node> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("Failed to read JSON5 file '{}'", path.display()))?;
        let data: BTreeMap<String, SerdeNode> = json5::from_str(&raw)
            .with_context(|| format!("Failed to parse JSON5 in '{}'", path.display()))?;
        let mut root = Node::new_dir("".to_string());
        for (k, v) in data {
            root.children.push(v.to_node(k));
        }
        Ok(root)
    }
}

impl ParseFile for TomlParser {
    fn parse_file(path: &Path) -> Result<Node> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read TOML file '{}'", path.display()))?;
        let data: BTreeMap<String, SerdeNode> = toml::from_str(&content)
            .with_context(|| format!("Failed to parse TOML in '{}'", path.display()))?;
        let mut root = Node::new_dir("".to_string());
        for (k, v) in data {
            root.children.push(v.to_node(k));
        }
        Ok(root)
    }
}

/// 并行解析输入文件，收集所有错误
pub fn parse_input_files_parallel(input_paths: &[PathBuf], jobs: usize) -> Result<Vec<Node>> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(if jobs == 0 { num_cpus::get() } else { jobs })
        .build()
        .context("Failed to create thread pool for parsing")?;

    pool.install(|| {
        input_paths
            .par_iter()
            .map(|input_path| {
                if !input_path.exists() {
                    bail!("Input file '{}' does not exist", input_path.display());
                }

                let ext = input_path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();

                match ext.as_str() {
                    "md" => MdParser::parse_file(input_path),
                    "yaml" | "yml" => YamlParser::parse_file(input_path),
                    "json" => JsonParser::parse_file(input_path),
                    "toml" => TomlParser::parse_file(input_path),
                    "json5" => Json5Parser::parse_file(input_path),
                    _ => bail!("Unsupported file extension '{}'", input_path.display()),
                }
            })
            .collect()
    })
}
