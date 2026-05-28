# quid-bot

Rust製の多機能パーソナルDiscord Bot。

## 機能

### AI質問 (`/ask`)

| コマンド | 概要 |
|---------|------|
| `/ask new <質問>` | 新しい会話スレッドを作成してAIに質問 |
| `/ask oneshot <質問> [look]` | 単発質問（`look:N` でチャンネル直近N件を参照） |
| `/ask save <名前>` | 会話をAI要約して名前付き保存（フル履歴削除でDB軽量化） |
| `/ask load <名前>` | 保存済み会話を復元（要約をコンテキストに注入） |
| `/ask saved` | 保存済み会話一覧 |
| `/ask pin` | スレッドの自動アーカイブを1週間に延長 |
| `/ask dispose` | スレッドをアーカイブ（保存済みなら履歴保持、未保存なら削除） |
| `/ask clear` | 現在のスレッドの会話履歴を削除 |
| `/ask usage` | AI利用量の統計（トークン数・モデル別） |
| `@Bot メッセージ` | メンションでoneshot応答（返信先メッセージを自動参照） |
| `@Bot !look:N メッセージ` | チャンネル直近N件の会話を参照して応答 |
| スレッド内の発言 | Bot作成スレッド内では自動でAI応答 |

OpenRouter (デフォルト) / OpenAI / Claude を切替可能。レスポンスにトークン数表示。AI応答待ち中は橙色「考え中...」Embedを表示→応答後に編集で差し替え。

### デイリーブリーフィング (`/today`)

| コマンド | 概要 |
|---------|------|
| `/today show` | 天気・ニュース・TODO・習慣・リマインダーをまとめて表示 |
| `/today city <都市名>` | 天気の対象都市を設定（サーバー単位） |
| `/today feeds <カテゴリ,...>` | ニュースカテゴリを設定（サーバー単位） |
| `/today subscribe <HH:MM> [チャンネル]` | 毎日の自動投稿を設定（管理者のみ） |
| `/today unsubscribe` | 自動投稿を解除 |

天気は [Open-Meteo API](https://open-meteo.com)（無料・キー不要）。

### ニュース (`/news`)

| コマンド | 概要 |
|---------|------|
| `/news show <カテゴリ>` | ニュース表示（tech/world/japan/business） |
| `/news show <カテゴリ> summary:True` | AI要約付きで表示 |
| `/news add <名前> <RSS URL>` | カスタムRSSフィード登録 |
| `/news remove <名前>` | カスタムフィード削除 |
| `/news list` | プリセット＋カスタムフィード一覧 |

### 日記 (`/diary`)

| コマンド | 概要 |
|---------|------|
| `/diary write` | 日記を書く（気分・タグ・完了タスク自動集計・公開/非公開） |
| `/diary edit [日付]` | 既存の日記を編集（上書き） |
| `/diary start` | 日記モード開始 — 以降の発言を自動収集 |
| `/diary end` | 日記モード終了 — 収集した発言をまとめて保存 |
| `/diary list` | 過去の日記一覧 |
| `/diary view <日付>` | 特定日の日記を表示 |
| `/diary search <キーワード>` | キーワードで日記を検索 |
| `/diary delete <日付>` | 日記を削除 |

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
| `/pomo start [分] [VCチャンネル] [notify_vc_members]` | タイマー開始 |
| `/pomo stop` | 中断 |
| `/pomo status` | 残り時間・進捗バー |

60秒ごとにリアルタイム進捗更新。`notify_vc_members:True` でVC参加者全員にメンション。

### リマインダー (`/remind`)

| コマンド | 概要 |
|---------|------|
| `/remind set <時間> <メッセージ>` | 一回限りリマインダー |
| `/remind repeat <頻度> <時刻> <メッセージ>` | 繰り返し（daily/weekly/monthly） |
| `/remind list` | 一覧 |
| `/remind delete <ID>` | 削除 |

時間指定: `30m`, `2h`, `1d`, `15:00`, `2025-06-01 09:00`

### VCアラーム (`/alarm`)

| コマンド | 概要 |
|---------|------|
| `/alarm set <時間> [VCチャンネル] [repeat]` | アラーム設定（リピート回数指定可、デフォルト3回） |
| `/alarm stop [ID]` | アラーム停止（ID省略で全停止、鳴動中・スヌーズ中を含む） |
| `/alarm snooze <ID> [分]` | スヌーズ（デフォルト5分） |
| `/alarm volume <0-100>` | アラーム音量設定（サーバー単位、デフォルト30%） |
| `/alarm list` | 一覧（🔔=鳴動中、⏰=待機中） |
| `/alarm delete <ID>` | 削除 |

時間になるとVCに接続して音声を指定回数リピート再生。ユーザーが移動していても現在のVCに追従。再生後に自動退出。

### 習慣トラッカー (`/habit`)

| コマンド | 概要 |
|---------|------|
| `/habit add <名前>` | 習慣を登録 |
| `/habit check <名前>` | 達成をチェック |
| `/habit list` | 一覧（ボタンUI付き） |
| `/habit stats <名前>` | streak・週間/月間達成率 |
| `/habit remove <名前>` | 削除 |

### 睡眠記録 (`/sleep`)

| コマンド | 概要 |
|---------|------|
| `/sleep start` | 就寝を記録 |
| `/sleep end [質] [メモ]` | 起床を記録（good/ok/bad） |
| `/sleep log <就寝> <起床>` | 過去分を手動記録 |
| `/sleep stats` | 週間/月間平均・就寝時刻傾向・グラフ |
| `/sleep goal <時間>` | 目標睡眠時間を設定（達成率追跡） |
| `/sleep history [件数]` | 直近の記録一覧 |

### 音楽 (`/music`)

| コマンド | 概要 |
|---------|------|
| `/music random [genre] [keyword]` | Spotifyからランダム1曲おすすめ |
| `/music genres` | 使用可能なジャンル一覧 |

ジャンル: pop, rock, jazz, k-pop, anime, lo-fi 等25種。アルバムアート・Spotifyリンク・30秒プレビュー付き。

### メモ/スニペット (`/memo`)

| コマンド | 概要 |
|---------|------|
| `/memo save <タイトル> <内容> [タグ] [言語]` | メモを保存（コードブロック対応） |
| `/memo get <ID>` | メモをIDで取得（シンタックスハイライト付き） |
| `/memo list` | メモ一覧 |
| `/memo search <キーワード>` | タイトル・内容・タグで検索 |
| `/memo delete <ID>` | メモを削除 |

### スレッドブックマーク (`/thread`)

| コマンド | 概要 |
|---------|------|
| `/thread save <名前>` | 現在のスレッドをブックマーク |
| `/thread list` | ブックマーク一覧 |
| `/thread delete <ID>` | ブックマーク削除 |

### サーバー設定 (`/settings`)

| コマンド | 概要 |
|---------|------|
| `/settings apikey <provider> <key>` | サーバー固有のAPIキーを設定（管理者のみ） |
| `/settings provider <name>` | AIプロバイダーを設定（openai/openrouter/anthropic） |
| `/settings model <name>` | AIモデルを設定 |
| `/settings show` | 現在の設定を表示（キーはマスク表示） |

サーバーごとにAPIキー・プロバイダー・モデルを設定可能。未設定の場合は `.env` / `config.toml` のグローバル設定にフォールバック。

### ヘルプ (`/quid-help`)

| コマンド | 概要 |
|---------|------|
| `/quid-help` | 全コマンドの概要一覧 |
| `/quid-help <コマンド名>` | 特定コマンドの詳細・サブコマンド一覧 |

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
SPOTIFY_CLIENT_ID=your_spotify_client_id    # optional, for /music
SPOTIFY_CLIENT_SECRET=your_spotify_secret   # optional, for /music
ENCRYPTION_KEY=your-32-char-secret-key!!!   # for API key encryption
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
- [feed-rs](https://github.com/feed-rs/feed-rs) — RSS/Atomフィードパーサー
- [Open-Meteo](https://open-meteo.com) — 天気API（無料・キー不要）
- [Spotify Web API](https://developer.spotify.com/) — 音楽レコメンド
- [aes-gcm](https://github.com/RustCrypto/AEADs) — APIキー暗号化（AES-256-GCM）

## ライセンス

MIT
