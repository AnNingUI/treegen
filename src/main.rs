use anyhow::{Context, Result};
use clap::Parser;
use std::{env, fs};
use treegen::{create_fs_parallel, parse_input_files_parallel, Args, CreateFsOptions, Node};
fn main() -> Result<()> {
    let args = Args::parse();

    let out_dir = args
        .out
        .unwrap_or_else(|| env::current_dir().expect("Failed to get current working directory"));

    if args.clean && out_dir.exists() {
        if args.verbose {
            eprintln!("Cleaning existing directory: {}", out_dir.display());
        }
        fs::remove_dir_all(&out_dir)
            .with_context(|| format!("Failed to remove directory '{}'", out_dir.display()))?;
    }

    if !args.clean {
        fs::create_dir_all(&out_dir).with_context(|| {
            format!("Failed to create output directory '{}'", out_dir.display())
        })?;
    }

    let mode = u32::from_str_radix(args.mode.trim_start_matches("0o"), 8)
        .context("Invalid mode format; use octal like 0o644")?;

    // 并行解析输入文件（收集所有错误）
    let parsed_nodes = parse_input_files_parallel(&args.input, args.jobs)?;

    let mut root = Node::new_dir("".to_string());
    for node in parsed_nodes {
        root.children.extend(node.children);
    }

    let opts = CreateFsOptions {
        dry_run: args.dry_run,
        verbose: args.verbose,
        force: args.force,
        mode,
        jobs: args.jobs,
        batch_log: args.batch_log,
    };
    // 使用并行文件创建
    create_fs_parallel(&out_dir, &root, &opts)?;

    if args.dry_run {
        println!("✅ Dry‐Run 完成，没有写入磁盘。");
    } else {
        println!("✅ 成功在 '{}' 生成文件树！", out_dir.display());
    }
    Ok(())
}
