# quid-bot

Rust製の多機能パーソナルDiscord Bot。

## 機能

| コマンド | 概要 |
|---------|------|
| `/ask` | AI質問（OpenRouter/OpenAI/Claude切替、スレッド単位で会話履歴保持） |
| `/diary` | 日報/日記（テンプレート+自由記述、公開/非公開選択） |
| `/pomo` | ポモドーロタイマー（メンション通知、オプトインVC通知） |
| `/remind` | リマインダー（一回限り+繰り返し対応） |
| `/alarm` | VCアラーム（VC接続して音声ファイル再生） |
| `/todo` | TODO管理（優先度・期限・日報自動集計） |
| `/habit` | 習慣トラッカー（コマンド+ボタンUI、streak・達成率・グラフ） |

## セットアップ

### 必要要件

- Rust 1.80+
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

### ビルド・実行

```bash
cargo build --release
./target/release/quid-bot
```

## 技術スタック

- [poise](https://github.com/serenity-rs/poise) — Discord botフレームワーク (serenity上)
- [sqlx](https://github.com/launchbadge/sqlx) — 非同期SQLite
- [songbird](https://github.com/serenity-rs/songbird) — Discord音声
- [reqwest](https://github.com/seanmonstar/reqwest) — HTTP client
- [tokio](https://github.com/tokio-rs/tokio) — 非同期ランタイム

## ライセンス

MIT
