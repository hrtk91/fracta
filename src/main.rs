mod commands;
mod compose;
mod config;
mod hooks;
mod state;
mod utils;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "fracta")]
#[command(about = "Worktree + compose helper CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// worktree を追加
    Add {
        /// worktree 名
        name: String,

        /// 新規ブランチを作成（オプション：基準ブランチ名を指定、省略時はHEADから作成）
        #[arg(short = 'b', long = "new-branch")]
        base_branch: Option<Option<String>>,
    },

    /// worktree を起動
    Up {
        /// worktree 名
        name: String,
    },

    /// worktree を再起動
    Restart {
        /// worktree 名
        name: String,
    },

    /// worktree を停止
    Down {
        /// worktree 名
        name: String,
    },

    /// worktree を削除
    #[command(alias = "rm")]
    Remove {
        /// worktree 名
        name: String,

        /// compose down の失敗を無視して削除を続行
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
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Add { name, base_branch } => {
            commands::add::execute(&name, base_branch)
        }
        Commands::Up { name } => {
            commands::up::execute(&name)
        }
        Commands::Restart { name } => {
            commands::restart::execute(&name)
        }
        Commands::Down { name } => {
            commands::down::execute(&name)
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
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
