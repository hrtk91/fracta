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
#[command(about = "Worktree + Lima VM + docker compose helper CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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

    /// worktree の状態を表示
    Ps {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,
    },

    /// 公開ポート一覧を表示
    Ports {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,

        /// 短い形式で表示
        #[arg(long)]
        short: bool,
    },

    /// worktree 一覧を表示
    #[command(alias = "list")]
    Ls,

    /// Lima VM にシェル接続（name省略時は現在のworktree）
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

    /// SSH ポートフォワードを開始
    Forward {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,

        /// ローカルポート
        local_port: u16,

        /// リモートポート（VM 内のポート）
        remote_port: u16,
    },

    /// SSH ポートフォワードを停止
    Unforward {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,

        /// ローカルポート（--all で全て停止）
        local_port: Option<u16>,

        /// 全てのポートフォワードを停止
        #[arg(long)]
        all: bool,
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
    Proxies,

    /// Playwright でブラウザを起動（SOCKS5 経由）
    Open {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,

        /// ブラウザ（chrome|firefox）
        #[arg(long, default_value = "chrome")]
        browser: String,

        /// 最初に開く URL
        #[arg(long, default_value = "about:blank")]
        url: String,
    },

    /// Playwright ブラウザを停止
    Close {
        /// worktree 名（省略時は現在ディレクトリの worktree）
        name: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Add { name, base_branch } => {
            commands::add::execute(&name, base_branch)
        }
        Commands::Up { name, no_sync_images } => {
            commands::up::execute(name.as_deref(), no_sync_images)
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
        Commands::Ps { name } => {
            commands::ps::execute(name.as_deref())
        }
        Commands::Ports { name, short } => {
            commands::ports::execute(name.as_deref(), short)
        }
        Commands::Ls => {
            commands::ls::execute()
        }
        Commands::Shell { name, shell, workdir, tty, command } => {
            commands::shell::execute(
                name.as_deref(),
                shell.as_deref(),
                workdir.as_deref(),
                tty,
                &command,
            )
        }
        Commands::Forward { name, local_port, remote_port } => {
            commands::forward::execute(name.as_deref(), local_port, remote_port)
        }
        Commands::Unforward { name, local_port, all } => {
            if all {
                commands::unforward::execute_all(name.as_deref())
            } else if let Some(port) = local_port {
                commands::unforward::execute(name.as_deref(), port)
            } else {
                eprintln!("Error: Either --all or local_port must be specified");
                std::process::exit(1);
            }
        }
        Commands::Proxy { name, port } => {
            commands::proxy::execute(name.as_deref(), port)
        }
        Commands::Unproxy { name } => {
            commands::unproxy::execute(name.as_deref())
        }
        Commands::Proxies => {
            commands::proxies::execute()
        }
        Commands::Open { name, browser, url } => {
            commands::open::execute(name.as_deref(), &browser, &url)
        }
        Commands::Close { name } => {
            commands::close::execute(name.as_deref())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
