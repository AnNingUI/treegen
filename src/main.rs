use anyhow::{bail, Context, Result};
use clap::Parser;
use regex::Regex;
use serde::Deserialize;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{collections::BTreeMap, env, fs, path::PathBuf};

/// CLI 参数定义
#[derive(Parser, Debug)]
#[command(name = "treegen")]
#[command(author = "AnNingUI <3533581512@qq.com>")]
#[command(version = "0.1.0")]
#[command(
    about = "Generate file/folder trees from Markdown/YAML/JSON/TOML/JSON5 specifications",
    long_about = None
)]
struct Args {
    /// 要解析的一个或多个输入文件（支持 .md/.yaml/.yml/.json/.toml/.json5）
    #[arg(required = true)]
    input: Vec<PathBuf>,

    /// 输出根目录（可选，默认是当前工作目录）
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// 仅预览将要创建的文件/目录，不写入磁盘
    #[arg(long)]
    dry_run: bool,

    /// 打印详细日志（每个文件/目录创建情况）
    #[arg(short, long)]
    verbose: bool,

    /// 如果输出目录已存在同名路径，先删除再创建（谨慎使用）
    #[arg(long)]
    clean: bool,

    /// 新建文件的权限（八进制，如 0o644，仅类 Unix 平台生效）
    #[arg(long, default_value = "0o644")]
    mode: String,
}

/// 节点类型：目录或文件
#[derive(Debug)]
enum NodeType {
    Dir,
    File,
}

/// 树节点结构
#[derive(Debug)]
struct Node {
    name: String,
    node_type: NodeType,
    children: Vec<Node>,
    content: Option<String>, // 用于 YAML/JSON/TOML/JSON5 中指定文件内容
}

impl Node {
    /// 构造一个文件节点（可携带内容）
    fn new_file(name: String, content: Option<String>) -> Self {
        Node {
            name,
            node_type: NodeType::File,
            children: Vec::new(),
            content,
        }
    }
    /// 构造一个空目录节点
    fn new_dir(name: String) -> Self {
        Node {
            name,
            node_type: NodeType::Dir,
            children: Vec::new(),
            content: None,
        }
    }
}

/// === Markdown 树状目录解析 ===
/// 示例：
/// project/
/// ├── src/
/// │   ├── main.rs
/// │   └── lib.rs
/// ├── Cargo.toml
/// └── README.md
fn parse_md_tree(lines: &[String]) -> Result<Node> {
    // 根节点（"" 表示从指定输出目录开始，不创建额外文件夹）
    let mut root = Node::new_dir("".to_string());

    // 栈：维护 (level, *mut Node) 以便附加子节点
    let mut stack: Vec<(usize, *mut Node)> = Vec::new();
    let root_ptr: *mut Node = &mut root as *mut Node;
    stack.push((0, root_ptr));

    // 正则匹配：捕获缩进(indent)、可选前缀(prefix)、以及名称(name)
    let re = Regex::new(r"^(?P<indent>(│   |    )*)(?P<prefix>├── |└── )?(?P<name>.+)$")?;

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let caps = re
            .captures(line)
            .with_context(|| format!("Line '{}' does not match Markdown tree format", line))?;

        // 计算 indent_blocks = 每 4 字符算一级
        let indent_str = caps.name("indent").map_or("", |m| m.as_str());
        let indent_blocks = indent_str.chars().count() / 4;

        // 如果有 prefix (“├── ” 或 “└── ”)，层级 = indent_blocks + 2；否则 = indent_blocks + 1
        let level = if caps.name("prefix").is_some() {
            indent_blocks + 2
        } else {
            indent_blocks + 1
        };

        let name = caps.name("name").unwrap().as_str().trim().to_string();
        let node_type = if name.ends_with('/') {
            NodeType::Dir
        } else {
            NodeType::File
        };

        let child = Node {
            name: name.clone(),
            node_type,
            children: Vec::new(),
            content: None, // 移除内容填充功能
        };

        // 弹出直到栈顶的 level < 当前 level
        while stack.last().unwrap().0 >= level {
            stack.pop();
        }
        // 此时栈顶即为父节点
        let parent_ptr = stack.last().unwrap().1;
        unsafe {
            let parent_ref: &mut Node = &mut *parent_ptr;
            parent_ref.children.push(child);
            let last_idx = parent_ref.children.len() - 1;
            if let NodeType::Dir = parent_ref.children[last_idx].node_type {
                // 如果新节点是目录，把它压入栈
                let child_ptr: *mut Node = &mut parent_ref.children[last_idx] as *mut Node;
                stack.push((level, child_ptr));
            }
        }
    }

    Ok(root)
}

/// === YAML/JSON/TOML 解析 ===
/// SerdeNode 用于反序列化：
/// - Str(String)：代表文件内容
/// - Map(BTreeMap<_, _>)：代表目录及其子结构
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SerdeNode {
    Str(String),
    Map(BTreeMap<String, SerdeNode>),
}

/// 将 SerdeNode 转为我们自己的 Node 结构
fn serde_to_node(name: String, snode: &SerdeNode) -> Node {
    match snode {
        SerdeNode::Str(content) => Node::new_file(name, Some(content.clone())),
        SerdeNode::Map(map) => {
            let mut dir = Node::new_dir(name);
            for (k, v) in map {
                dir.children.push(serde_to_node(k.clone(), v));
            }
            dir
        }
    }
}

/// 从 YAML 文件中解析出 Node 树
fn parse_yaml_file(path: &PathBuf) -> Result<Node> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read YAML file '{}'", path.display()))?;
    let data: BTreeMap<String, SerdeNode> = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML in '{}'", path.display()))?;
    let mut root = Node::new_dir("".to_string());
    for (k, v) in data {
        root.children.push(serde_to_node(k, &v));
    }
    Ok(root)
}

/// 从 JSON 文件中解析出 Node 树
fn parse_json_file(path: &PathBuf) -> Result<Node> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read JSON file '{}'", path.display()))?;
    let data: BTreeMap<String, SerdeNode> = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON in '{}'", path.display()))?;
    let mut root = Node::new_dir("".to_string());
    for (k, v) in data {
        root.children.push(serde_to_node(k, &v));
    }
    Ok(root)
}

/// 从 TOML 文件中解析出 Node 树
fn parse_toml_file(path: &PathBuf) -> Result<Node> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read TOML file '{}'", path.display()))?;
    let data: BTreeMap<String, SerdeNode> = toml::from_str(&content)
        .with_context(|| format!("Failed to parse TOML in '{}'", path.display()))?;

    let mut root = Node::new_dir("".to_string());
    for (key, value) in data {
        root.children.push(parse_toml_node(key, &value));
    }
    Ok(root)
}

fn parse_toml_node(name: String, snode: &SerdeNode) -> Node {
    match snode {
        SerdeNode::Str(content) => Node::new_file(name, Some(content.clone())),
        SerdeNode::Map(map) => {
            let mut dir = Node::new_dir(name);
            for (key, value) in map {
                dir.children.push(parse_toml_node(key.clone(), value));
            }
            dir
        }
    }
}

/// dedent(): 去除多行字符串的首尾空行 + 公共缩进，保持内容整体对齐
fn dedent(s: &str) -> String {
    // 1. 按行拆分，去掉首尾纯空行
    let mut lines: Vec<&str> = s.lines().collect();
    // 去掉前导空行
    while !lines.is_empty() && lines.first().unwrap().trim().is_empty() {
        lines.remove(0);
    }
    // 去掉末尾空行
    while !lines.is_empty() && lines.last().unwrap().trim().is_empty() {
        lines.pop();
    }
    if lines.is_empty() {
        return String::new();
    }
    // 2. 找到所有非空行的最小缩进数（以空格计）
    let mut min_indent = usize::MAX;
    for &line in &lines {
        if line.trim().is_empty() {
            continue;
        }
        let count = line.chars().take_while(|c| *c == ' ').count();
        if count < min_indent {
            min_indent = count;
        }
    }
    if min_indent == usize::MAX {
        min_indent = 0;
    }
    // 3. 对每行去除前 min_indent 个空格
    let dedented: Vec<String> = lines
        .into_iter()
        .map(|line| {
            if line.len() >= min_indent {
                line[min_indent..].to_string()
            } else {
                line.trim_start().to_string()
            }
        })
        .collect();
    dedented.join("\n")
}

/// === JSON5 格式解析 ===
/// 支持：
///  - 反引号（`…`）包裹多行字符串
///  - 单/双引号字符串、无引号键、注释、末尾逗号等 JSON5 特性
///  - 写法示例 (structure.json5)：
///    ```json5
///    // 顶层就是一个对象
///    {
///      my_project: {
///        src: {
///          "main.rs": `
///            fn main() {
///                println!("Hello from JSON5!");
///            }
///          `,
///          "lib.rs": ""
///        },
///        "Cargo.toml": `
///    [package]
///    name = "my_project"
///    version = "0.1.0"
///    `,
///        "README.md": `
///    # My Project
///
///    这是示例项目，通过 JSON5 定义生成。
///    `
///      }
///    }
///    ```
///  直接用 `json5::from_str` 解析时，内部会保留原样的多行文本，我们再对其 dedent 后输出。
fn parse_json5_file(path: &PathBuf) -> Result<Node> {
    // 1. 读取整个 .json5 文件内容
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read JSON5 file '{}'", path.display()))?;

    // 2. 我们需要先把所有反引号包裹的多行内容 dedent 后再交给 json5 解析。
    //    简单思路：扫描整个 raw，将 `…` 之间的内容先提取、dedent、再放回 raw 中。
    let mut output = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '`' {
            // 收集反引号内的内容
            let mut content = String::new();
            while let Some(&next_ch) = chars.peek() {
                chars.next();
                if next_ch == '`' {
                    break;
                } else {
                    content.push(next_ch);
                }
            }
            // dedent 之后再放到 output：用三引号包裹以便 JSON5 理解多行？
            // 但 json5 本身也支持反引号，此处只要保证「缩进对齐」，让 JSON5 解析时拿到干净的多行文本即可。
            let dedented = dedent(&content); // 修复反引号包裹内容的缩进问题
            output.push('`');
            output.push_str(&dedented);
            output.push('`');
        } else {
            output.push(ch);
        }
    }

    // 3. 用 json5 解析成 BTreeMap<String, SerdeNode>
    let data: BTreeMap<String, SerdeNode> = json5::from_str(&output)
        .with_context(|| format!("Failed to parse JSON5 in '{}'", path.display()))?;

    // 4. 转为 Node 树
    let mut root = Node::new_dir("".to_string());
    for (k, v) in data {
        root.children.push(serde_to_node(k, &v));
    }
    Ok(root)
}

/// 从 Markdown 文件中解析 Node 树
fn parse_md_file(path: &PathBuf) -> Result<Node> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read Markdown file '{}'", path.display()))?;
    let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let sanitized_lines: Vec<String> = lines
        .iter()
        .map(|line| line.replace(":", "_")) // 修复文件名语法问题
        .collect();
    parse_md_tree(&sanitized_lines)
}

/// === 递归在磁盘上创建目录和文件 ===
fn create_fs(base: &PathBuf, node: &Node, dry_run: bool, verbose: bool, _mode: u32) -> Result<()> {
    // 如果 name 为空，则 base 本身；否则 base/<name>
    let path = if node.name.is_empty() {
        base.clone()
    } else {
        base.join(&node.name)
    };

    match node.node_type {
        NodeType::Dir => {
            if dry_run {
                if verbose {
                    println!("[Dry-Run] Create directory: {}", path.display());
                }
            } else {
                if verbose {
                    println!("Create directory: {}", path.display());
                }
                fs::create_dir_all(&path)
                    .with_context(|| format!("Failed to create directory '{}'", path.display()))?;
            }
            for child in node.children.iter() {
                create_fs(&path, child, dry_run, verbose, _mode)
                    .with_context(|| format!("Failed under directory '{}'", path.display()))?;
            }
        }
        NodeType::File => {
            if let Some(parent) = path.parent() {
                if !dry_run {
                    fs::create_dir_all(parent).ok();
                } else if verbose {
                    println!("[Dry-Run] Ensure parent dirs for: {}", path.display());
                }
            }
            if dry_run {
                if verbose {
                    println!("[Dry-Run] Create file: {}", path.display());
                }
            } else {
                if verbose {
                    println!("Create file: {}", path.display());
                }
                if let Some(content) = &node.content {
                    fs::write(&path, content)
                        .with_context(|| format!("Failed to write file '{}'", path.display()))?;
                } else {
                    fs::write(&path, "").with_context(|| {
                        format!("Failed to create empty file '{}'", path.display())
                    })?;
                }
                #[cfg(unix)]
                {
                    fs::set_permissions(&path, fs::Permissions::from_mode(_mode)).with_context(
                        || format!("Failed to set permissions for '{}'", path.display()),
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    // 解析命令行参数
    let args = Args::parse();

    // 确定输出目录：如果指定了 --out，就用它；否则用当前工作目录
    let out_dir = if let Some(dir) = args.out.clone() {
        dir
    } else {
        env::current_dir().context("Failed to get current working directory")?
    };

    // 如果 --clean 并且 out_dir 存在，则先删除
    if args.clean && out_dir.exists() {
        if args.verbose {
            println!("Cleaning existing directory: {}", out_dir.display());
        }
        fs::remove_dir_all(&out_dir)
            .with_context(|| format!("Failed to remove directory '{}'", out_dir.display()))?;
    }

    // 确保输出目录存在
    if !args.clean {
        fs::create_dir_all(&out_dir).with_context(|| {
            format!("Failed to create output directory '{}'", out_dir.display())
        })?;
    }

    // 解析 mode，如 "0o644" -> 0o644
    let mode = u32::from_str_radix(args.mode.trim_start_matches("0o"), 8)
        .context("Invalid mode format; use octal like 0o644")?;

    // 根节点：合并所有输入文件解析结果
    let mut root = Node::new_dir("".to_string());

    for input_path in &args.input {
        if !input_path.exists() {
            bail!("Input file '{}' does not exist", input_path.display());
        }
        let ext = input_path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let parsed = match ext.as_str() {
            "md" => parse_md_file(input_path)?,
            "yaml" | "yml" => parse_yaml_file(input_path)?,
            "json" => parse_json_file(input_path)?,
            "toml" => parse_toml_file(input_path)?,
            "json5" => parse_json5_file(input_path)?,
            _ => bail!("Unsupported file extension '{}'", input_path.display()),
        };
        // 合并子节点
        root.children.extend(parsed.children);
    }

    // 递归在 out_dir 下创建目录/文件
    create_fs(&out_dir, &root, args.dry_run, args.verbose, mode)?;

    if args.dry_run {
        println!("✅ Dry‐Run 完成，没有写入磁盘。");
    } else {
        println!("✅ 成功在 '{}' 生成文件树！", out_dir.display());
    }
    Ok(())
}
