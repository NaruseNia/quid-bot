# CLAUDE.md

## プロジェクト概要

quid-bot: Rust製パーソナルDiscord Bot（友人数人で利用）

## 技術スタック

- 言語: Rust (edition 2024)
- Discord: poise 0.6 (serenity 0.12ベース)
- DB: SQLite (sqlx)
- 音声: songbird
- HTTP: reqwest
- 非同期: tokio
- 設定: .env (dotenv) + config.toml (toml)

## ビルド・実行

```bash
cargo build          # ビルド
cargo run            # 実行（.env必須）
cargo test           # テスト
cargo clippy         # lint
```

## ディレクトリ構成

```
src/
├── main.rs          # エントリポイント、Bot初期化
├── config.rs        # 設定読み込み (.env + config.toml)
├── db.rs            # DB初期化、マイグレーション
├── error.rs         # エラー型定義
└── commands/
    ├── mod.rs       # コマンド登録
    ├── ask.rs       # /ask — AI質問
    ├── diary.rs     # /diary — 日報/日記
    ├── pomo.rs      # /pomo — ポモドーロ
    ├── remind.rs    # /remind — リマインダー
    ├── alarm.rs     # /alarm — VCアラーム
    ├── todo.rs      # /todo — TODO管理
    └── habit.rs     # /habit — 習慣トラッカー
migrations/
└── 001_init.sql     # 初期スキーマ
assets/              # 音声ファイル等
```

## コーディング規約

- コマンド名: 英語 (`/ask`, `/todo` 等)
- Bot応答: 日本語
- エラーメッセージ: 日本語でユーザーフレンドリーに
- コミットメッセージ: 英語、Conventional Commits (`feat:`, `fix:`, `docs:` 等)
- コミット粒度: 機能単位

## DB

- SQLite、sqlxのマイグレーション機能でスキーマ管理
- `data/quid.db` に保存

## 設定

- APIキー・トークン → `.env`（gitignore対象）
- Bot動作設定 → `config.toml`

## 主要な設計判断

- poise の `Data` 構造体でアプリケーション状態を共有（DB pool, HTTP client, config）
- 各コマンドは `src/commands/` 内に1ファイル1モジュールで分離
- ポモドーロ・リマインダーは tokio::spawn でバックグラウンドタスク管理
- AI APIはOpenRouterをデフォルト、コマンド引数でOpenAI/Claude直接指定も可
- 日報とTODOは連携：日報作成時に当日完了タスクを自動集計
