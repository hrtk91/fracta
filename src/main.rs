mod commands;
mod config;
mod hooks;
mod lima;
mod images;
mod state;
mod utils;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "fracta")]
#[command(
    about = "Worktree + Lima VM + docker compose helper CLI",
    long_about = "Worktree + Lima VM + docker compose helper CLI\n\nDocs/README:\n  https://github.com/hrtk91/fracta\n\nConfig:\n  fracta.toml, fracta.*.toml (main repo -> worktree, later files override)\n  hooks: use \"vm:\" or \"limactl:\" prefix to run inside VM"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum VmCommands {
    /// Lima VM を起動（compose は起動しない）
    Start {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,
    },

    /// Lima VM を停止（compose は停止しない）
    Stop {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,
    },

    /// Lima VM にシェル接続（compose は触らない）
    Shell {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,

        /// shell interpreter（例: /bin/bash）
        #[arg(long)]
        shell: Option<String>,

        /// working directory
        #[arg(long)]
        workdir: Option<String>,

        /// TTY を明示（true/false）
        #[arg(long)]
        tty: Option<bool>,

        /// 実行コマンド（limactl shell と同じ形式、'--' 以降を渡す）
        #[arg(trailing_var_arg = true)]
        command: Vec<String>,
    },

    /// VM 一覧を表示
    #[command(alias = "ls")]
    List,
}

#[derive(Subcommand)]
enum BrowserCommands {
    /// Playwright でブラウザを起動（必要ならSOCKS5を自動起動）
    Open {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,

        /// ブラウザ（chrome|firefox）
        #[arg(long, default_value = "chrome")]
        browser: String,

        /// 最初に開く URL
        #[arg(long, default_value = "about:blank")]
        url: String,

        /// SOCKS5 ローカルポート（省略時は自動割当）
        #[arg(long)]
        proxy_port: Option<u16>,
    },

    /// Playwright ブラウザを停止
    Close {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,
    },

    /// SOCKS5 プロキシを開始
    Proxy {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,

        /// ローカルポート（省略時は自動割当）
        #[arg(long)]
        port: Option<u16>,
    },

    /// SOCKS5 プロキシを停止
    Unproxy {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,
    },

    /// SOCKS5 プロキシ一覧
    #[command(alias = "list")]
    Status,
}

#[derive(Subcommand)]
enum Commands {
    /// worktree と Lima VM を追加
    Add {
        /// worktree 名
        name: String,

        /// 新規ブランチを作成（オプション：基準ブランチ名を指定、省略時はHEADから作成）
        #[arg(short = 'b', long = "new-branch")]
        base_branch: Option<Option<String>>,
    },

    /// docker compose を起動（VM が停止中なら起動）
    Up {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,

        /// 事前のイメージ同期を無効化
        #[arg(long)]
        no_sync_images: bool,

        /// docker compose の並列ビルドを無効化
        #[arg(long)]
        no_parallel_build: bool,

        /// VM 内のローカルコピーで compose を実行（ビルド高速化向け）
        #[arg(long)]
        vm_build_copy: bool,

        /// VM 内のコピー先ルートディレクトリ（例: /tmp/fracta-build）
        #[arg(long)]
        vm_build_dir: Option<String>,
    },

    /// worktree を再起動
    Restart {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,
    },

    /// docker compose を停止
    Down {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,

        /// Lima VM も停止する
        #[arg(long)]
        vm: bool,
    },

    /// worktree と Lima VM を削除
    #[command(alias = "rm")]
    Remove {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,

        /// エラーを無視して削除を続行
        #[arg(long)]
        force: bool,

        /// Lima VM のみ削除（worktree は残す）
        #[arg(long)]
        vm_only: bool,

        /// worktree のみ削除（VM は残す）
        #[arg(long)]
        worktree_only: bool,
    },

    /// worktree の状態と公開ポートを表示
    Status {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,
    },

    /// Lima VM を直接操作
    Vm {
        #[command(subcommand)]
        command: VmCommands,
    },

    /// ブラウザ・SOCKS5 操作
    Browser {
        #[command(subcommand)]
        command: BrowserCommands,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Add { name, base_branch } => {
            commands::add::execute(&name, base_branch)
        }
        Commands::Up {
            name,
            no_sync_images,
            no_parallel_build,
            vm_build_copy,
            vm_build_dir,
        } => {
            commands::up::execute(
                name.as_deref(),
                no_sync_images,
                no_parallel_build,
                vm_build_copy,
                vm_build_dir.as_deref(),
            )
        }
        Commands::Restart { name } => {
            commands::restart::execute(name.as_deref())
        }
        Commands::Down { name, vm } => {
            commands::down::execute(name.as_deref(), vm)
        }
        Commands::Remove { name, force, vm_only, worktree_only } => {
            commands::remove::execute(name.as_deref(), force, vm_only, worktree_only)
        }
        Commands::Status { name } => commands::status::execute(name.as_deref()),
        Commands::Vm { command } => match command {
            VmCommands::Start { name } => commands::vm::start(name.as_deref()),
            VmCommands::Stop { name } => commands::vm::stop(name.as_deref()),
            VmCommands::Shell { name, shell, workdir, tty, command } => commands::vm::shell(
                name.as_deref(),
                shell.as_deref(),
                workdir.as_deref(),
                tty,
                &command,
            ),
            VmCommands::List => commands::vm::list(),
        },
        Commands::Browser { command } => match command {
            BrowserCommands::Open { name, browser, url, proxy_port } => {
                commands::browser::open(
                    name.as_deref(),
                    &browser,
                    &url,
                    proxy_port,
                )
            }
            BrowserCommands::Close { name } => commands::browser::close(name.as_deref()),
            BrowserCommands::Proxy { name, port } => {
                commands::browser::proxy(name.as_deref(), port)
            }
            BrowserCommands::Unproxy { name } => commands::browser::unproxy(name.as_deref()),
            BrowserCommands::Status => commands::browser::status(),
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
