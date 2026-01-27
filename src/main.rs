mod commands;
mod config;
mod hooks;
mod lima;
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
    },

    /// worktree を再起動
    Restart {
        /// worktree 名
        name: String,
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
        /// worktree 名
        name: String,

        /// エラーを無視して削除を続行
        #[arg(long)]
        force: bool,
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

    /// Lima VM にシェル接続
    Shell {
        /// worktree 名
        name: String,
    },

    /// SSH ポートフォワードを開始
    Forward {
        /// worktree 名
        name: String,

        /// ローカルポート
        local_port: u16,

        /// リモートポート（VM 内のポート）
        remote_port: u16,
    },

    /// SSH ポートフォワードを停止
    Unforward {
        /// worktree 名
        name: String,

        /// ローカルポート（--all で全て停止）
        local_port: Option<u16>,

        /// 全てのポートフォワードを停止
        #[arg(long)]
        all: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Add { name, base_branch } => {
            commands::add::execute(&name, base_branch)
        }
        Commands::Up { name } => {
            commands::up::execute(name.as_deref())
        }
        Commands::Restart { name } => {
            commands::restart::execute(&name)
        }
        Commands::Down { name, vm } => {
            commands::down::execute(name.as_deref(), vm)
        }
        Commands::Remove { name, force } => {
            commands::remove::execute(&name, force)
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
        Commands::Shell { name } => {
            commands::shell::execute(&name)
        }
        Commands::Forward { name, local_port, remote_port } => {
            commands::forward::execute(&name, local_port, remote_port)
        }
        Commands::Unforward { name, local_port, all } => {
            if all {
                commands::unforward::execute_all(&name)
            } else if let Some(port) = local_port {
                commands::unforward::execute(&name, port)
            } else {
                eprintln!("Error: Either --all or local_port must be specified");
                std::process::exit(1);
            }
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
