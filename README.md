# fracta - Worktree環境管理CLI

`fracta`は、git worktreeとdocker-composeを組み合わせて、独立した開発環境を簡単に作成・管理するCLIツールです。

## 🎯 コンセプト

**複数ブランチで同時に開発。それぞれに独立したDocker環境**

- ブランチごとに完全に隔離された開発環境
- ポート衝突を気にせず複数環境を同時起動
- ブランチ切り替えでコンテナ再起動不要
- レビュー時も環境をそのまま維持して切り替え

## 📋 前提条件

- Git（git worktree機能を使用）
- Docker Compose（Docker Compose V2）

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
# 1. 新しいworktree環境を追加
fracta add feature-A

# 2. 環境を起動
fracta up feature-A

# 3. アクセス
# Backend:  http://localhost:13910
# Frontend School:  http://localhost:13901
# Frontend Student: http://localhost:13903
# Frontend Medical: http://localhost:13905

# 4. 停止
fracta down feature-A

# 5. 完全削除
fracta remove feature-A
```

## 📖 コマンド一覧

#### `add <name>`

新しいworktree環境を追加します。

```bash
# 基本的な使い方（既存ブランチを使用）
fracta add feature-A

# 新規ブランチを作成してworktreeを追加
fracta add feature-new -b main      # mainブランチから新規ブランチ作成
fracta add feature-new2 -b          # HEADから新規ブランチ作成
```

**オプション：**
- `-b, --new-branch [BASE_BRANCH]`: 新規ブランチを作成（BASE_BRANCHを指定すると、そのブランチから作成。省略時はHEADから作成）

**処理内容：**
- git worktree作成（既存ブランチまたは新規ブランチ）
- ポートオフセット自動計算
- composeファイル生成（`.fracta/compose.generated.yml`）
- state.jsonに登録

#### `up <name>`

worktree環境を起動します。

```bash
fracta up feature-A
```

**処理内容：**
- docker compose up -d を実行
- 起動後に公開ポート一覧を表示

#### `down <name>`

worktree環境を停止します。

```bash
fracta down feature-A
```

**処理内容：**
- docker compose down を実行

#### `restart <name>`

worktree環境を再起動します。

```bash
fracta restart feature-A
```

**処理内容：**
- docker compose restart を実行

#### `remove <name>`

worktree環境を完全削除します。

```bash
fracta remove feature-A
# または
fracta rm feature-A
```

**オプション：**
- `--force`: compose down の失敗を無視して削除を続行

**処理内容：**
- docker compose down を実行
- 生成されたcomposeファイルを削除（`.fracta/compose.generated.yml`）
- git worktree remove を実行
- state.jsonから削除

#### `ps [name]`

worktree環境の状態を表示します。

```bash
fracta ps              # 現在ディレクトリのworktreeの状態
fracta ps feature-A    # 特定worktreeの状態
```

#### `ports [name]`

公開ポート一覧を表示します。

```bash
fracta ports              # 現在ディレクトリのworktreeのポート
fracta ports feature-A     # 特定worktreeのポート
fracta ports --short       # 短い形式で表示
```

#### `ls`

worktree一覧を表示します。

```bash
fracta ls
# または
fracta list
```

### 設定ファイル（fracta.toml）

プロジェクトルートに`fracta.toml`を作成すると、compose baseファイルのパスをカスタマイズできます。

**例：**

```toml
compose_base = "docker-compose.yml"  # デフォルト値
```

設定ファイルが存在しない場合は、デフォルトで`docker-compose.yml`が使用されます。

## 🔗 Hooks

`.fracta/hooks/`ディレクトリにスクリプトを配置すると、各コマンド実行時に自動的にフックを実行できます。

### 対応フック

- `pre_add`, `post_add` - worktree追加前後
- `pre_up`, `post_up` - 起動前後
- `pre_restart`, `post_restart` - 再起動前後
- `pre_down`, `post_down` - 停止前後
- `pre_remove`, `post_remove` - 削除前後

### 実行条件

- ファイルが存在し、実行権限（+x）がある場合のみ実行
- 存在しない、または実行権限がない場合はスキップ

### 環境変数

フック実行時に以下の環境変数が利用できます：

- `FRACTA_NAME` - worktree名
- `FRACTA_PATH` - worktreeの絶対パス
- `MAIN_REPO` - メインリポジトリの絶対パス
- `PORT_OFFSET` - ポートオフセット（数値）
- `COMPOSE_BASE` - compose baseファイルのパス
- `COMPOSE_OVERRIDE` - 生成されたcomposeファイルのパス（`.fracta/compose.generated.yml`）

### 実行ディレクトリ

- `pre_add`: メインリポジトリ
- その他のフック: worktreeディレクトリ

### 例

```bash
# .fracta/hooks/post_add を作成
#!/bin/bash
echo "Worktree $FRACTA_NAME added at $FRACTA_PATH"
cd "$FRACTA_PATH"
npm install

# 実行権限を付与
chmod +x .fracta/hooks/post_add
```

## 🏗️ ポート割り当て

ポートは自動的に計算されます。

| 環境 | オフセット | Backend | Frontend School | Frontend Student | Frontend Medical | DB |
|------|-----------|---------|----------------|-----------------|-----------------|-----|
| main | 0 | 12910 | 12901 | 12903 | 12905 | 12911 |
| feature-A | 1000 | 13910 | 13901 | 13903 | 13905 | 13911 |
| feature-B | 2000 | 14910 | 14901 | 14903 | 14905 | 14911 |

※ オフセットはworktree名のハッシュ値から自動計算されます。

## 📁 ディレクトリ構造

```
school_health_dx/
├── .fracta/
│   ├── state.json              # worktree状態管理
│   └── hooks/                  # フックスクリプト（オプション）
│       ├── pre_add
│       ├── post_add
│       └── ...
├── fracta.toml                  # 設定ファイル（オプション）
└── ../
    ├── school_health_dx-feature-A/   # worktree
    │   └── .fracta/
    │       └── compose.generated.yml
    └── school_health_dx-feature-B/   # worktree
        └── .fracta/
            └── compose.generated.yml
```

## 🔧 トラブルシューティング

### ポート衝突が発生する

`fracta ls`で既存のworktreeを確認し、不要なものを`fracta remove`で削除してください。

### docker compose upが失敗する

- dockerデーモンが起動しているか確認
- worktreeディレクトリで`docker compose --project-directory . -f .fracta/compose.generated.yml up`を直接実行してエラー内容を確認

### worktreeが削除できない

```bash
# 手動削除
cd ../school_health_dx-feature-A
docker compose --project-directory . -f .fracta/compose.generated.yml down --volumes
cd ..
rm -rf school_health_dx-feature-A
git worktree prune
```

その後、`.fracta/state.json`から該当エントリを手動削除してください。

または、`--force`オプションを使用して強制削除することもできます：

```bash
fracta remove feature-A --force
```

## 📝 ライセンス

（プロジェクトのライセンスに準拠）
