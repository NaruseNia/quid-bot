use super::{Context, Error};
use poise::serenity_prelude::CreateEmbed;

struct CommandInfo {
    name: &'static str,
    emoji: &'static str,
    short: &'static str,
    subcommands: &'static [(&'static str, &'static str)],
    extras: &'static str,
}

const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        name: "ask",
        emoji: "🤖",
        short: "AI質問（スレッド会話・単発・メンション対応）",
        subcommands: &[
            ("new <質問>", "新しい会話スレッドを作成してAIに質問"),
            ("oneshot <質問>", "単発質問（スレッドなし・履歴なし）"),
            ("clear", "現在のスレッドの会話履歴を削除"),
            ("dispose", "スレッドをアーカイブ+ロックして履歴削除"),
            ("usage", "AI利用量の統計を表示"),
        ],
        extras: "**その他の使い方**\n\
                 ・`@Bot メッセージ` → oneshot応答\n\
                 ・Bot作成スレッド内の発言 → 自動でAI応答\n\n\
                 **オプション**\n\
                 ・`provider` — openrouter / openai / anthropic\n\
                 ・`model` — モデル名を直接指定",
    },
    CommandInfo {
        name: "diary",
        emoji: "📔",
        short: "日記（自由記述・発言自動収集）",
        subcommands: &[
            ("write <内容>", "日記を書く（気分・タグ・公開/非公開オプション）"),
            ("start", "日記モード開始 — 以降の発言を自動収集"),
            ("end", "日記モード終了 — 収集した発言をまとめて保存"),
            ("list [件数]", "過去の日記一覧"),
            ("view <日付>", "特定日の日記を表示"),
            ("search <キーワード>", "キーワードで日記を検索"),
        ],
        extras: "完了タスクは日記に自動集計される。",
    },
    CommandInfo {
        name: "todo",
        emoji: "📋",
        short: "TODO管理（優先度・期限・日記連携）",
        subcommands: &[
            ("add <タスク名>", "タスク追加（priority / due_date オプション）"),
            ("list [show_completed]", "タスク一覧"),
            ("done <ID>", "タスクを完了"),
            ("delete <ID>", "タスクを削除"),
        ],
        extras: "優先度: 🔴high / 🟡medium / 🟢low\n期限: YYYY-MM-DD 形式",
    },
    CommandInfo {
        name: "pomo",
        emoji: "🍅",
        short: "ポモドーロタイマー",
        subcommands: &[
            ("start [分] [VCチャンネル]", "タイマー開始（デフォルト25分）"),
            ("stop", "中断"),
            ("status", "残り時間・進捗バー・今日のセッション数"),
        ],
        extras: "VCチャンネルを指定すると完了時に音声で通知。",
    },
    CommandInfo {
        name: "remind",
        emoji: "⏰",
        short: "リマインダー（一回限り+繰り返し）",
        subcommands: &[
            ("set <時間> <メッセージ>", "一回限りリマインダー"),
            ("repeat <頻度> <時刻> <メッセージ>", "繰り返し（daily/weekly/monthly）"),
            ("list", "一覧"),
            ("delete <ID>", "削除"),
        ],
        extras: "**時間の指定方法**\n\
                 ・`30m` `2h` `1d` — 相対指定\n\
                 ・`15:00` — 今日または明日の時刻\n\
                 ・`2025-06-01 09:00` — 絶対指定",
    },
    CommandInfo {
        name: "alarm",
        emoji: "🔔",
        short: "VCアラーム（音声再生・スヌーズ対応）",
        subcommands: &[
            ("set <時間> <VCチャンネル>", "アラーム設定"),
            ("snooze <ID> [分]", "スヌーズ（デフォルト5分）"),
            ("list", "一覧"),
            ("delete <ID>", "削除"),
        ],
        extras: "時間になるとVCに接続して音声再生。再生後に自動退出。",
    },
    CommandInfo {
        name: "habit",
        emoji: "🎯",
        short: "習慣トラッカー（streak・達成率・ボタンUI）",
        subcommands: &[
            ("add <名前>", "習慣を登録"),
            ("check <名前>", "達成をチェック"),
            ("list", "一覧（ボタンUI付き）"),
            ("stats <名前>", "streak・週間/月間達成率"),
            ("remove <名前>", "削除"),
        ],
        extras: "",
    },
    CommandInfo {
        name: "sleep",
        emoji: "😴",
        short: "睡眠記録（就寝/起床・質・統計・目標）",
        subcommands: &[
            ("start", "就寝を記録"),
            ("end [質] [メモ]", "起床を記録（good/ok/bad）"),
            ("log <就寝> <起床> [質] [メモ]", "手動で過去分を記録"),
            ("stats", "週間/月間平均・就寝時刻傾向・グラフ"),
            ("goal <時間>", "目標睡眠時間を設定"),
            ("history [件数]", "直近の記録一覧"),
        ],
        extras: "",
    },
    CommandInfo {
        name: "news",
        emoji: "📰",
        short: "ニュース（プリセット+カスタムRSS、AI要約対応）",
        subcommands: &[
            ("show <カテゴリ>", "ニュースを表示（tech/world/japan/business）"),
            ("show <カテゴリ> summary:True", "AI要約付きで表示"),
            ("add <名前> <RSS URL>", "カスタムRSSフィードを登録"),
            ("remove <名前>", "カスタムフィードを削除"),
            ("list", "登録フィード一覧"),
        ],
        extras: "",
    },
    CommandInfo {
        name: "today",
        emoji: "☀️",
        short: "デイリーブリーフィング（天気・ニュース・TODO・習慣）",
        subcommands: &[
            ("show", "今日のブリーフィングを表示"),
            ("city <都市名>", "天気の対象都市を設定（サーバー単位）"),
            ("feeds <カテゴリ,...>", "ニュースカテゴリを設定（サーバー単位）"),
            ("subscribe <HH:MM> [チャンネル]", "毎日の自動投稿を設定（管理者のみ）"),
            ("unsubscribe", "自動投稿を解除（管理者のみ）"),
        ],
        extras: "**天気**: Open-Meteo API（無料・キー不要）\n\
                 **ニュース**: RSS → AI要約\n\
                 **TODO/習慣/リマインダー**: 手動実行時のみ表示",
    },
];

/// コマンド一覧・使い方を表示
#[poise::command(slash_command, rename = "quid-help")]
pub async fn quid_help(
    ctx: Context<'_>,
    #[description = "詳細を見るコマンド名"] command: Option<String>,
) -> Result<(), Error> {
    if let Some(name) = command {
        show_detail(ctx, &name).await
    } else {
        show_overview(ctx).await
    }
}

async fn show_overview(ctx: Context<'_>) -> Result<(), Error> {
    let mut desc = String::new();
    for cmd in COMMANDS {
        desc.push_str(&format!(
            "{} **/{name}** — {short}\n",
            cmd.emoji,
            name = cmd.name,
            short = cmd.short,
        ));
    }
    desc.push_str("\n`/quid-help <コマンド名>` で詳しい使い方を表示");

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("📖 quid-bot ヘルプ")
                .description(desc)
                .color(0x5865F2),
        ),
    )
    .await?;
    Ok(())
}

async fn show_detail(ctx: Context<'_>, name: &str) -> Result<(), Error> {
    let name = name.strip_prefix('/').unwrap_or(name);

    let Some(cmd) = COMMANDS.iter().find(|c| c.name == name) else {
        ctx.say(format!("コマンド `{}` が見つかりません。`/quid-help` で一覧を確認してください。", name))
            .await?;
        return Ok(());
    };

    let subs: String = cmd
        .subcommands
        .iter()
        .map(|(sub, desc)| format!("**/{} {}** — {}", cmd.name, sub, desc))
        .collect::<Vec<_>>()
        .join("\n");

    let mut embed = CreateEmbed::new()
        .title(format!("{} /{}", cmd.emoji, cmd.name))
        .description(cmd.short)
        .color(0x5865F2)
        .field("サブコマンド", subs, false);

    if !cmd.extras.is_empty() {
        embed = embed.field("補足", cmd.extras, false);
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}
