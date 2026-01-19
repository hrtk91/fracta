# fracta 変更仕様 (draft)

## 目的
- 名称を fracta に統一し、git worktree と docker compose のラップに責務を絞る
- プロジェクト固有テンプレート生成と DB 操作は本体から外し、hooks に委譲する
- 生成物は .fracta 配下に寄せ、git 差分を出さない

## スコープ
- git worktree の作成/削除
- docker compose の up/down ラップ
- compose 生成ファイル作成（port/container_name 反映）
- hooks 実行

## 非スコープ
- DB copy / dump / restore
- docker image/build の最適化
- プロジェクト固有テンプレート生成

## 設定

### 設定ファイル
- `MAIN_REPO/fracta.toml`
- ない場合はデフォルト値を使用

### MAIN_REPO 解決
- `git rev-parse --git-common-dir` の親ディレクトリを `MAIN_REPO` とする

### compose base
- `compose_base`（デフォルト: `docker-compose.yml`）
- worktree ルートからの相対パスとして解釈（絶対パス指定も可）
- 各 worktree に存在する前提（通常はリポジトリに commit 済み）
- ファイルが見つからない場合はエラーで終了

## コマンド

### add
```
fracta add <name> [-b [base]]
```
- `git worktree add` を実行
- `-b` を付けた場合は新規ブランチ作成（`base` 指定でそのブランチから）
- port offset を計算して compose 生成ファイルを作成
- hooks: `pre_add` -> `post_add`

### up
```
fracta up <name>
```
- `docker compose up -d` を実行
- 実行後に公開ポート一覧を表示
- hooks: `pre_up` -> `post_up`

### restart
```
fracta restart <name>
```
- `docker compose restart` を実行
- hooks: `pre_restart` -> `post_restart`

### down
```
fracta down <name>
```
- `docker compose down` を実行
- hooks: `pre_down` -> `post_down`

### remove / rm
```
fracta remove <name>
fracta rm <name>
fracta remove <name> --force
```
- `docker compose down` を実行
- `git worktree remove` を実行
- hooks: `pre_remove` -> `post_remove`
- 生成済み compose ファイルを削除
  - `down` は `--project-directory <worktree> -f <worktree>/.fracta/compose.generated.yml` を使用
- `--force` 指定時は `compose` の失敗や不足を警告にして削除を続行

### ps
```
fracta ps
fracta ps <name>
```
- `docker compose ps` を実行
- `<name>` 指定時はその worktree を対象
- `<name>` 省略時は現在ディレクトリが worktree の場合のみ実行

### ports
```
fracta ports
fracta ports <name>
fracta ports <name> --short
```
- 生成済み compose から公開ポート一覧を表示
- `--short` 指定時はヘッダなしのタブ区切り（service/host/target）

### ls / list
```
fracta ls
fracta list
```
- worktree 一覧を表示

## Hooks

### 置き場所
- `MAIN_REPO/.fracta/hooks/<hook>`

### 実行条件
- ファイルが存在し、実行権限 (+x) がある場合のみ実行
- それ以外はスキップ

### hook 名 (最小)
- `pre_add`, `post_add`
- `pre_up`, `post_up`
- `pre_restart`, `post_restart`
- `pre_down`, `post_down`
- `pre_remove`, `post_remove`

### 実行ディレクトリ
- `pre_add`: `MAIN_REPO`
- `post_add`: `WORKTREE_PATH`
- `pre_up` / `post_up` / `pre_restart` / `post_restart` / `pre_down` / `post_down` / `pre_remove` / `post_remove`: `WORKTREE_PATH`

### 環境変数
- `FRACTA_NAME`（worktree 名）
- `FRACTA_PATH`（worktree 絶対パス）
- `MAIN_REPO`（MAIN_REPO 絶対パス）
- `PORT_OFFSET`（数値）
- `COMPOSE_BASE`（worktree ルートからの相対パス or 絶対パス）
- `COMPOSE_OVERRIDE`（`WORKTREE/.fracta/compose.generated.yml` の絶対パス）

### 失敗時の扱い
- hook が非 0 で終了したら fracta も失敗扱いで停止
- stdout/stderr はそのまま表示

## Compose Generated

### 生成場所
- `WORKTREE/.fracta/compose.generated.yml`
- git 管理外

### 利用方法
```
docker compose --project-directory <worktree> -f <worktree>/.fracta/compose.generated.yml up -d
```

### 変更対象
- `services.*.ports` の host 側のみ
- `services.*.container_name`（既存の値に `-<name>` を付与）

### 変換ルール
- `HOST:CONTAINER` -> `(HOST + offset):CONTAINER`
- `IP:HOST:CONTAINER` -> `IP:(HOST + offset):CONTAINER`
- `HOST:CONTAINER/PROTO` -> `(HOST + offset):CONTAINER/PROTO`
- long syntax は `published` が数値（数値文字列含む）の場合のみ offset 適用
- env 参照（`${VAR}`/`${VAR-DEFAULT}`）は環境変数と compose base と同階層の `.env` を参照し、数値なら offset 適用

### 変更しないケース
- host が数値以外（env 参照で解決できない場合など）
- ポート範囲 (例: `8000-8010:8000-8010`)
- コンテナ側のみ指定 (例: `"3000"`)
- long syntax で `published` が数値以外、または未指定

### port offset 算出
- name が `main` または空文字の場合は `0`
- それ以外は安定ハッシュで 1000..9000 の範囲 (1000 刻み)
  - 例: `((hash % 9) + 1) * 1000`
- 既に使用中の offset がある場合は、ハッシュ値のバケットから順に空きを探す

## 状態管理
- `MAIN_REPO/.fracta/state.json` に最低限の状態を保存（git 管理外）
  - `name`, `path`, `port_offset`, `branch` など

## 互換性メモ
- 旧コマンド（`create/start/stop/cleanup`）は非対応
- `fracta` のみを正式コマンドとする

## 将来検討
- `~/.fracta` によるグローバル設定
