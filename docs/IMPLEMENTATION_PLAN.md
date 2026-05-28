# 実装計画

## フェーズ 1: プロジェクト骨格

- [x] Cargo.toml（依存関係）
- [x] ディレクトリ構成
- [x] .env.example / config.toml.example
- [x] .gitignore
- [ ] config.rs — 設定読み込み
- [ ] db.rs — SQLite初期化 + マイグレーション
- [ ] error.rs — エラー型
- [ ] main.rs — Bot起動、poise Framework構築
- [ ] migrations/001_init.sql — 全テーブル定義

## フェーズ 2: /ask — AI質問

- [ ] OpenRouter API連携（デフォルト）
- [ ] OpenAI / Claude API 切替オプション
- [ ] スレッド単位の会話履歴保持
- [ ] ストリーミング風応答（メッセージ編集で擬似表示）

## フェーズ 3: /todo — TODO管理

- [ ] `/todo add <title>` — タスク追加
- [ ] `/todo list` — 一覧表示
- [ ] `/todo done <id>` — 完了
- [ ] `/todo delete <id>` — 削除
- [ ] 優先度（高/中/低）
- [ ] 期限設定
- [ ] 日報集計用クエリ

## フェーズ 4: /diary — 日報/日記

- [ ] テンプレート入力（モーダル）
- [ ] 自由記述入力
- [ ] 公開/非公開選択
- [ ] 完了タスク自動集計表示
- [ ] `/diary list` — 過去の日報一覧
- [ ] `/diary view <date>` — 特定日の日報表示

## フェーズ 5: /pomo — ポモドーロ

- [ ] `/pomo start [minutes]` — タイマー開始
- [ ] `/pomo stop` — 中断
- [ ] `/pomo status` — 残り時間
- [ ] メンション通知（作業終了/休憩終了）
- [ ] セッション数カウント
- [ ] オプトインVC通知

## フェーズ 6: /remind — リマインダー

- [ ] `/remind <time> <message>` — 一回限り
- [ ] `/remind repeat <cron> <message>` — 繰り返し
- [ ] `/remind list` — 一覧
- [ ] `/remind delete <id>` — 削除
- [ ] 自然言語風の時間指定（`30m`, `2h`, `tomorrow 9:00`）

## フェーズ 7: /habit — 習慣トラッカー

- [ ] `/habit add <name>` — 習慣登録
- [ ] `/habit check [name]` — コマンドでチェック
- [ ] ボタンUI付き日次メッセージ
- [ ] streak表示
- [ ] 週次/月次達成率
- [ ] `/habit stats [name]` — 統計表示

## フェーズ 8: /alarm — VCアラーム

- [ ] `/alarm set <time>` — アラーム設定
- [ ] `/alarm list` — 一覧
- [ ] `/alarm delete <id>` — 削除
- [ ] VCに接続して音声ファイル再生
- [ ] songbird統合

## DBスキーマ設計

### conversations（AI会話履歴）
| カラム | 型 | 説明 |
|-------|-----|------|
| id | INTEGER PK | |
| user_id | TEXT | Discord user ID |
| thread_id | TEXT | Discord thread ID |
| role | TEXT | "user" or "assistant" |
| content | TEXT | メッセージ内容 |
| model | TEXT | 使用モデル |
| created_at | DATETIME | |

### todos
| カラム | 型 | 説明 |
|-------|-----|------|
| id | INTEGER PK | |
| user_id | TEXT | Discord user ID |
| guild_id | TEXT | Discord guild ID |
| title | TEXT | タスク名 |
| priority | TEXT | "high" / "medium" / "low" |
| due_date | DATETIME | 期限（nullable） |
| completed | BOOLEAN | |
| completed_at | DATETIME | 完了日時（nullable） |
| created_at | DATETIME | |

### diaries
| カラム | 型 | 説明 |
|-------|-----|------|
| id | INTEGER PK | |
| user_id | TEXT | Discord user ID |
| guild_id | TEXT | Discord guild ID |
| content | TEXT | 日報内容（JSON: テンプレート or 自由） |
| is_public | BOOLEAN | 公開フラグ |
| date | DATE | 対象日 |
| created_at | DATETIME | |

### pomodoro_sessions
| カラム | 型 | 説明 |
|-------|-----|------|
| id | INTEGER PK | |
| user_id | TEXT | |
| guild_id | TEXT | |
| duration_min | INTEGER | 作業時間（分） |
| completed | BOOLEAN | |
| started_at | DATETIME | |
| finished_at | DATETIME | nullable |

### reminders
| カラム | 型 | 説明 |
|-------|-----|------|
| id | INTEGER PK | |
| user_id | TEXT | |
| guild_id | TEXT | |
| channel_id | TEXT | |
| message | TEXT | |
| remind_at | DATETIME | |
| cron_expr | TEXT | nullable（繰り返し用） |
| is_recurring | BOOLEAN | |
| is_active | BOOLEAN | |
| created_at | DATETIME | |

### habits
| カラム | 型 | 説明 |
|-------|-----|------|
| id | INTEGER PK | |
| user_id | TEXT | |
| guild_id | TEXT | |
| name | TEXT | 習慣名 |
| is_active | BOOLEAN | |
| created_at | DATETIME | |

### habit_logs
| カラム | 型 | 説明 |
|-------|-----|------|
| id | INTEGER PK | |
| habit_id | INTEGER FK | |
| checked_at | DATE | チェック日 |

### alarms
| カラム | 型 | 説明 |
|-------|-----|------|
| id | INTEGER PK | |
| user_id | TEXT | |
| guild_id | TEXT | |
| channel_id | TEXT | VCチャンネルID |
| alarm_at | DATETIME | |
| is_active | BOOLEAN | |
| created_at | DATETIME | |
