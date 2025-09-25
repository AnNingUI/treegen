use anyhow::Result;
use std::path::PathBuf;
use std::time::Instant;

use treegen::{create_fs_parallel, CreateFsOptions, Node};

/// 构造一个包含 N 个文件的 Node 树，每个文件大小为 file_size 字节
fn generate_tree(num_files: usize, file_size: usize) -> Node {
    let mut root = Node::new_dir("bench".to_string());
    let content = "A".repeat(file_size);

    for i in 0..num_files {
        let name = format!("file_{i}.txt");
        root.children
            .push(Node::new_file(name, Some(content.clone())));
    }
    root
}

fn run_bench(num_files: usize, file_size: usize, jobs: usize) -> Result<()> {
    let root = generate_tree(num_files, file_size);
    let out_dir = PathBuf::from(format!("bench_out_{num_files}_{file_size}_{jobs}"));

    let opts = CreateFsOptions {
        dry_run: false,
        verbose: false,
        force: true,
        mode: 0o644,
        jobs,
        batch_log: 0,
    };

    let start = Instant::now();
    create_fs_parallel(&out_dir, &root, &opts)?;
    let elapsed = start.elapsed();

    let total_bytes = num_files * file_size;
    let mb = total_bytes as f64 / (1024.0 * 1024.0);
    let secs = elapsed.as_secs_f64();
    let throughput = mb / secs;

    println!(
        "Files: {:>6}, Size: {:>6} KB, Threads: {:>2} | Time: {:>6.2} s | {:.2} MB/s",
        num_files,
        file_size / 1024,
        jobs,
        secs,
        throughput
    );

    Ok(())
}

fn main() -> Result<()> {
    // 测试不同规模
    let configs = vec![
        (1000, 1024),        // 1k 个文件，每个 1KB
        (10_000, 1024),      // 1w 个文件，每个 1KB
        (1000, 1024 * 1024), // 1k 个文件，每个 1MB
    ];

    let threads = vec![1, 4, 8];

    for &(num_files, file_size) in &configs {
        for &j in &threads {
            run_bench(num_files, file_size, j)?;
        }
    }

    Ok(())
}
