# quid-bot

Rust製の多機能パーソナルDiscord Bot。

## 機能

### AI質問 (`/ask`)

| コマンド | 概要 |
|---------|------|
| `/ask new <質問>` | 新しい会話スレッドを作成してAIに質問 |
| `/ask oneshot <質問>` | 単発質問（スレッド作成なし・履歴保存なし） |
| `/ask clear` | 現在のスレッドの会話履歴を削除 |
| `/ask dispose` | スレッドをアーカイブ+ロックして履歴削除 |
| `@Bot メッセージ` | メンションでoneshot応答 |
| スレッド内の発言 | Bot作成スレッド内では自動でAI応答 |

OpenRouter (デフォルト) / OpenAI / Claude を切替可能。

### 日記 (`/diary`)

| コマンド | 概要 |
|---------|------|
| `/diary write` | 日記を書く（気分・タグ・完了タスク自動集計・公開/非公開） |
| `/diary start` | 日記モード開始 — 以降の発言を自動収集 |
| `/diary end` | 日記モード終了 — 収集した発言をまとめて保存 |
| `/diary list` | 過去の日記一覧 |
| `/diary view <日付>` | 特定日の日記を表示 |
| `/diary search <キーワード>` | キーワードで日記を検索 |

### TODO管理 (`/todo`)

| コマンド | 概要 |
|---------|------|
| `/todo add <タスク名>` | タスク追加（優先度・期限オプション） |
| `/todo list` | タスク一覧 |
| `/todo done <ID>` | タスクを完了 |
| `/todo delete <ID>` | タスクを削除 |

完了タスクは日記に自動集計。

### ポモドーロ (`/pomo`)

| コマンド | 概要 |
|---------|------|
| `/pomo start [分] [VCチャンネル]` | タイマー開始（VC通知オプション） |
| `/pomo stop` | 中断 |
| `/pomo status` | 残り時間・進捗バー |

### リマインダー (`/remind`)

| コマンド | 概要 |
|---------|------|
| `/remind set <時間> <メッセージ>` | 一回限りリマインダー |
| `/remind repeat <頻度> <時刻> <メッセージ>` | 繰り返し（daily/weekly/monthly） |
| `/remind list` | 一覧 |
| `/remind delete <ID>` | 削除 |

### VCアラーム (`/alarm`)

| コマンド | 概要 |
|---------|------|
| `/alarm set <時間> <VCチャンネル>` | アラーム設定 |
| `/alarm snooze <ID> [分]` | スヌーズ（デフォルト5分） |
| `/alarm list` | 一覧 |
| `/alarm delete <ID>` | 削除 |

時間になるとVCに接続して音声再生。再生後に自動退出。

### 習慣トラッカー (`/habit`)

| コマンド | 概要 |
|---------|------|
| `/habit add <名前>` | 習慣を登録 |
| `/habit check <名前>` | 達成をチェック |
| `/habit list` | 一覧（ボタンUI付き） |
| `/habit stats <名前>` | streak・週間/月間達成率 |
| `/habit remove <名前>` | 削除 |

## セットアップ

### 必要要件

- Rust 1.85+ (edition 2024)
- SQLite 3

### 設定

```bash
cp .env.example .env
cp config.toml.example config.toml
```

`.env` にDiscordトークンとAPIキーを設定:

```env
DISCORD_TOKEN=your_discord_bot_token
OPENROUTER_API_KEY=your_openrouter_api_key
OPENAI_API_KEY=your_openai_api_key        # optional
ANTHROPIC_API_KEY=your_anthropic_api_key    # optional
```

`config.toml` でBot動作を設定（AIモデル、ポモドーロ時間、音声ファイルパス等）。

### Discord Developer Portal

1. Bot作成 → TOKEN取得
2. **Privileged Gateway Intents** → MESSAGE CONTENT INTENT を ON
3. OAuth2 URL Generator → `bot` + `applications.commands` スコープで招待

### 音声ファイル

VCアラーム・ポモドーロ通知用の音声ファイルを配置:

```
assets/alarm.mp3
assets/pomo.mp3
```

パスは `config.toml` の `[audio]` セクションで変更可能。

### ビルド・実行

```bash
cargo build --release
./target/release/quid-bot
```

## 技術スタック

- [poise](https://github.com/serenity-rs/poise) — Discord botフレームワーク (serenity上)
- [sqlx](https://github.com/launchbadge/sqlx) — 非同期SQLite
- [songbird](https://github.com/serenity-rs/songbird) — Discord音声（VC接続・再生）
- [reqwest](https://github.com/seanmonstar/reqwest) — HTTP client
- [tokio](https://github.com/tokio-rs/tokio) — 非同期ランタイム

## ライセンス

MIT
