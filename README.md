# quid-bot

Rust製の多機能パーソナルDiscord Bot。

## 機能

| コマンド | 概要 |
|---------|------|
| `/ask` | AI質問（OpenRouter/OpenAI/Claude切替、スレッド単位で会話履歴保持） |
| `/diary write` | 日記を書く（気分・タグ・完了タスク自動集計・公開/非公開） |
| `/diary start/end` | 日記モード（発言を自動収集→まとめて日記に保存） |
| `/diary list/view/search` | 過去の日記の閲覧・検索 |
| `/pomo start [minutes] [vc_channel]` | ポモドーロタイマー（メンション通知、オプトインVC通知） |
| `/remind set/repeat` | リマインダー（一回限り+繰り返し対応） |
| `/alarm set/snooze` | VCアラーム（VC接続して音声再生、スヌーズ対応） |
| `/todo add/list/done/delete` | TODO管理（優先度・期限・日記自動集計） |
| `/habit add/check/list/stats` | 習慣トラッカー（コマンド+ボタンUI、streak・達成率） |

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
