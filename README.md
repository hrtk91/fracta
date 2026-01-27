# fracta - Worktree + Lima VM 環境管理CLI

`fracta`は、git worktree と Lima VM を組み合わせて、ブランチごとに独立した Docker Compose 環境を作成・管理するCLIツールです。

## 🎯 コンセプト

**複数ブランチで同時に開発。それぞれに独立したVM + Compose環境**

- worktreeごとに専用の Lima VM を作成
- VM 内で docker compose を実行するため、ホスト側のポート衝突を避けられる
- 必要なサービスだけを SSH ポートフォワードで公開
- ブランチ切り替え時も環境を維持してそのまま切り替え

## 📋 前提条件

- Git（git worktree 機能を使用）
- Lima（`limactl`）

> macOS + Lima を前提としています。

## 🚀 Quickstart

### インストール

```bash
# GitHubリポジトリから直接インストール
cargo install --git https://github.com/hrtk91/fracta

# またはローカルでビルド
git clone https://github.com/hrtk91/fracta.git
cd fracta
cargo install --path .
```

### 基本的な使い方

```bash
# 1. 新しいworktree + Lima VM を追加
fracta add feature-A

# 2. VM内で docker compose を起動（VMが停止中なら自動起動）
fracta up feature-A

# 3. VM 内で公開されているポートを確認
fracta ports feature-A

# 4. 必要なサービスをローカルへフォワード
fracta forward feature-A 18080 8080
# → http://localhost:18080 でアクセス

# 5. 停止（--vm で VM も停止）
fracta down feature-A --vm

# 6. 完全削除（worktree + VM）
fracta remove feature-A
```

## 📖 コマンド一覧

#### `add <name>`

worktree と Lima VM を追加します。

```bash
# 既存ブランチを使用
fracta add feature-A

# 新規ブランチを作成してworktreeを追加
fracta add feature-new -b main  # main ブランチから作成
fracta add feature-new2 -b      # HEAD から作成
```

**オプション：**
- `-b, --new-branch [BASE_BRANCH]`: 新規ブランチを作成

**処理内容：**
- git worktree 作成（既存ブランチまたは新規ブランチ）
- Lima VM 作成（起動は `up` で実行）
- `.fracta/state.json` に登録

#### `up [name]`

VM 内で docker compose を起動します（VM が停止中なら起動）。`name` 省略時は現在ディレクトリの worktree を対象にします。

```bash
fracta up feature-A
# worktree 内なら省略可能
fracta up
```

#### `down [name]`

VM 内で docker compose を停止します。`name` 省略時は現在ディレクトリの worktree を対象にします。

```bash
fracta down feature-A
fracta down feature-A --vm  # VM も停止
# worktree 内なら省略可能
fracta down
```

**オプション：**
- `--vm`: Lima VM も停止

#### `restart [name]`

worktree を再起動します。

```bash
fracta restart feature-A
# worktree 内なら省略可能
fracta restart
```

#### `remove <name>`

worktree と Lima VM を完全削除します。

```bash
fracta remove feature-A
# または
fracta rm feature-A
```

**オプション：**
- `--force`: エラーを無視して削除を続行
- `--vm-only`: Lima VM のみ削除（worktreeは残す）
- `--worktree-only`: worktreeのみ削除（VMは残す）

#### `ps [name]`

worktree の状態を表示します。

```bash
fracta ps             # 現在ディレクトリの worktree
fracta ps feature-A   # 特定 worktree
```

#### `ports [name]`

公開ポート（VM 内の compose とフォワード状況）を表示します。

```bash
fracta ports
fracta ports feature-A
fracta ports --short   # フォワードのみ（local/remote）
```

#### `ls`

worktree 一覧を表示します。

```bash
fracta ls
# または
fracta list
```

#### `shell <name>`

Lima VM にシェル接続します。

```bash
fracta shell feature-A
```

**オプション（limactl shell と同じインターフェース）**:
- `--shell <PATH>`: shell interpreter（例: `/bin/bash`）
- `--workdir <PATH>`: working directory
- `--tty <true|false>`: TTY を明示

```bash
fracta shell feature-A -- ls -la
fracta shell feature-A --shell /bin/bash --workdir /home -- pwd
fracta shell feature-A --tty false -- ls -la
```

#### `forward [name] <local_port> <remote_port>`

SSH ポートフォワードを開始します。

```bash
fracta forward feature-A 18080 8080
# worktree 内なら省略可能
fracta forward 18080 8080
```

#### `unforward [name] [local_port]`

SSH ポートフォワードを停止します。

```bash
fracta unforward feature-A 18080
fracta unforward feature-A --all
# worktree 内なら省略可能
fracta unforward 18080
```

## 🧦 SOCKS5 プロキシ + Playwright

`fracta` は Lima VM への SSH ダイナミックフォワード（SOCKS5）を提供します。  
これにより VM 内の任意のポートへ、ブラウザ/Playwright からまとめてアクセスできます。

### 前提

- Node.js（`node` コマンド）
- Playwright（ホスト側にインストール済み）
  - 例: `npm i -g playwright` または `npm i -D playwright`

### proxy

```bash
# SOCKS5 を開始（ポート自動割当: 1080-1099）
fracta proxy feature-A

# ポート指定
fracta proxy feature-A --port 1081
```

### open / close

```bash
# Playwright で Chrome を起動（SOCKS5 経由）
fracta open feature-A --url http://localhost:12901

# Firefox で起動
fracta open feature-A --browser firefox

# 停止
fracta close feature-A
```

> `proxy/open/close` は `name` 省略時、現在ディレクトリの worktree を対象にします。

## ⚙️ 設定ファイル（fracta.toml）

プロジェクトルートに `fracta.toml` を作成すると、compose base ファイルのパスや registry mirror を指定できます。

```toml
compose_base = "docker-compose.yml"  # デフォルト値
registry_mirror = "http://host.lima.internal:5000"
```

- `compose_base` は worktree からの相対パス、または絶対パスを指定できます。
- `registry_mirror` は `fracta add` 時に作成される VM テンプレートに反映されます。

## 🔗 Hooks

`.fracta/hooks/` にスクリプトを配置すると、各コマンド実行時にフックを自動実行できます。

### 対応フック

- `pre_add`, `post_add` - worktree追加前後
- `pre_up`, `post_up` - 起動前後
- `pre_restart`, `post_restart` - 再起動前後
- `pre_down`, `post_down` - 停止前後
- `pre_remove`, `post_remove` - 削除前後

### 環境変数

- `FRACTA_NAME` - worktree名
- `FRACTA_PATH` - worktreeの絶対パス
- `MAIN_REPO` - メインリポジトリの絶対パス
- `PORT_OFFSET` - 互換用（v2では常に 0）
- `COMPOSE_BASE` - compose base ファイルのパス
- `COMPOSE_OVERRIDE` - v2 では `COMPOSE_BASE` と同じ

## 🔌 ポートフォワード

`fracta`はホストのポートを自動で割り当てません。必要なサービスのみを手動でフォワードします。

```bash
# VM 内で公開されているポートを確認
fracta ports feature-A

# ローカル 18080 -> VM 8080 をフォワード
fracta forward feature-A 18080 8080

# 停止
fracta unforward feature-A 18080
```

`fracta`はフォワード済みポートを `state.json` に記録し、同じローカルポートの重複を防ぎます。

## 📁 ディレクトリ構造

```
repo/
├── .fracta/
│   ├── state.json            # worktree状態管理
│   └── hooks/                # フックスクリプト（任意）
│       ├── pre_add
│       ├── post_add
│       └── ...
├── fracta.toml               # 設定ファイル（任意）
└── ../
    ├── repo-feature-A/       # worktree
    └── repo-feature-B/       # worktree
```

> Lima VM は `~/.lima/fracta-<name>/` に作成されます。

## 🔧 トラブルシューティング

### Lima が見つからない

```bash
brew install lima
```

### VM が起動していない

```bash
fracta up feature-A
```

### ポートにアクセスできない

- `fracta ports` で VM 内の公開ポートを確認
- `fracta forward` でローカルにフォワード

### compose base が見つからない

- `docker-compose.yml` が worktree に存在するか確認
- `fracta.toml` の `compose_base` を修正

### compose が失敗する

`fracta shell` で VM に入り、worktree ディレクトリから直接 `docker compose` を実行してエラー内容を確認してください。

## 📝 ライセンス

（プロジェクトのライセンスに準拠）
