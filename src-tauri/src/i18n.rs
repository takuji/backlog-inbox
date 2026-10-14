//! バックエンドが生成するユーザー向け文字列(OS通知・トレイ・エラー)の翻訳。

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Ja,
    En,
}

impl Lang {
    pub fn from_code(code: &str) -> Option<Lang> {
        match code {
            "ja" => Some(Lang::Ja),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::Ja => "ja",
            Lang::En => "en",
        }
    }

    /// OSのロケールから既定言語を推定する(日本語なら Ja、それ以外は En)。
    pub fn detect_default() -> Lang {
        match sys_locale::get_locale() {
            Some(l) if l.to_lowercase().starts_with("ja") => Lang::Ja,
            _ => Lang::En,
        }
    }
}

/// 通知理由コードを説明文にする(送信者に続く述部)。
pub fn reason_text(reason: i64, lang: Lang) -> &'static str {
    match (lang, reason) {
        (Lang::Ja, 1) => "課題の担当者に設定しました",
        (Lang::Ja, 2) => "課題にコメントしました",
        (Lang::Ja, 3) => "課題を追加しました",
        (Lang::Ja, 4) => "課題を更新しました",
        (Lang::Ja, 5) => "ファイルを追加しました",
        (Lang::Ja, 6) => "プロジェクトメンバーに追加しました",
        (Lang::Ja, 10) => "プルリクエストの担当者に設定しました",
        (Lang::Ja, 11) => "プルリクエストにコメントしました",
        (Lang::Ja, 12) => "プルリクエストを追加しました",
        (Lang::Ja, 13) => "プルリクエストを更新しました",
        (Lang::Ja, _) => "通知を送りました",
        (Lang::En, 1) => "assigned an issue to you",
        (Lang::En, 2) => "commented on an issue",
        (Lang::En, 3) => "added an issue",
        (Lang::En, 4) => "updated an issue",
        (Lang::En, 5) => "added a file",
        (Lang::En, 6) => "added you to a project",
        (Lang::En, 10) => "assigned a pull request to you",
        (Lang::En, 11) => "commented on a pull request",
        (Lang::En, 12) => "added a pull request",
        (Lang::En, 13) => "updated a pull request",
        (Lang::En, _) => "sent you a notification",
    }
}

/// 新着通知トーストの本文(「送信者 + 理由」)。
pub fn toast_new_body(sender: &str, reason: i64, lang: Lang) -> String {
    let reason = reason_text(reason, lang);
    match lang {
        Lang::Ja => format!("{}さんが{}", sender, reason),
        Lang::En => format!("{} {}", sender, reason),
    }
}

pub fn toast_unread_summary(count: i64, lang: Lang) -> String {
    match lang {
        Lang::Ja => format!("未読の通知が{}件あります", count),
        Lang::En => format!("You have {} unread notification(s)", count),
    }
}

pub fn toast_more(extra: usize, lang: Lang) -> String {
    match lang {
        Lang::Ja => format!("ほか{}件の新しい通知があります", extra),
        Lang::En => format!("and {} more new notification(s)", extra),
    }
}

pub fn tray_tooltip(unread: i64, lang: Lang) -> String {
    match (lang, unread) {
        (Lang::Ja, 0) => "Backlog Inbox".to_string(),
        (Lang::Ja, n) => format!("Backlog Inbox — 未読 {} 件", n),
        (Lang::En, 0) => "Backlog Inbox".to_string(),
        (Lang::En, n) => format!("Backlog Inbox — {} unread", n),
    }
}

pub fn menu_open(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "Backlog Inbox を開く",
        Lang::En => "Open Backlog Inbox",
    }
}

pub fn menu_refresh(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "今すぐ更新",
        Lang::En => "Refresh now",
    }
}

pub fn menu_quit(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "終了",
        Lang::En => "Quit",
    }
}

// --- エラーメッセージ ---

pub fn err_not_configured(lang: Lang) -> String {
    match lang {
        Lang::Ja => "Backlogの接続設定がありません。".into(),
        Lang::En => "No Backlog connection is configured.".into(),
    }
}

pub fn err_api_key_required(lang: Lang) -> String {
    match lang {
        Lang::Ja => "APIキーを入力してください。".into(),
        Lang::En => "Enter your API key.".into(),
    }
}

pub fn err_space_url(lang: Lang) -> String {
    match lang {
        Lang::Ja => "スペースのURL（例: example.backlog.com）を入力してください。".into(),
        Lang::En => "Enter your space URL (e.g. example.backlog.com).".into(),
    }
}

pub fn err_connect(detail: &str, lang: Lang) -> String {
    match lang {
        Lang::Ja => format!("Backlogへの接続に失敗しました: {}", detail),
        Lang::En => format!("Failed to connect to Backlog: {}", detail),
    }
}

pub fn err_auth(lang: Lang) -> String {
    match lang {
        Lang::Ja => "認証に失敗しました。APIキーを確認してください。".into(),
        Lang::En => "Authentication failed. Check your API key.".into(),
    }
}

pub fn err_not_found(lang: Lang) -> String {
    match lang {
        Lang::Ja => {
            "スペースまたはリソースが見つかりません。スペースURLを確認してください。".into()
        }
        Lang::En => "Space or resource not found. Check your space URL.".into(),
    }
}

pub fn err_api(status: u16, lang: Lang) -> String {
    match lang {
        Lang::Ja => format!("Backlog APIがエラーを返しました ({})", status),
        Lang::En => format!("The Backlog API returned an error ({}).", status),
    }
}

pub fn err_parse(detail: &str, lang: Lang) -> String {
    match lang {
        Lang::Ja => format!("Backlog APIの応答を解析できませんでした: {}", detail),
        Lang::En => format!("Could not parse the Backlog API response: {}", detail),
    }
}

pub fn err_secure_store(detail: &str, lang: Lang) -> String {
    match lang {
        Lang::Ja => format!("セキュアストアにアクセスできませんでした: {}", detail),
        Lang::En => format!("Could not access the secure store: {}", detail),
    }
}

pub fn err_save_key(detail: &str, lang: Lang) -> String {
    match lang {
        Lang::Ja => format!("APIキーの保存に失敗しました: {}", detail),
        Lang::En => format!("Failed to save the API key: {}", detail),
    }
}

pub fn err_save_settings(detail: &str, lang: Lang) -> String {
    match lang {
        Lang::Ja => format!("設定の保存に失敗しました: {}", detail),
        Lang::En => format!("Failed to save settings: {}", detail),
    }
}

pub fn err_clear_key(detail: &str, lang: Lang) -> String {
    match lang {
        Lang::Ja => format!("APIキーの削除に失敗しました: {}", detail),
        Lang::En => format!("Failed to delete the API key: {}", detail),
    }
}

/// settings.json を消すことも無効化することもできなかったときのメッセージ。
/// APIキーは既に消えている一方で設定ファイルが残っているという食い違った
/// 状態なので、「保存に失敗」ではなく解除の文脈で伝える。
pub fn err_clear_settings_file(detail: &str, lang: Lang) -> String {
    match lang {
        Lang::Ja => format!(
            "APIキーは削除しましたが、設定ファイルを削除できませんでした: {}",
            detail
        ),
        Lang::En => format!(
            "The API key was deleted, but the settings file could not be removed: {}",
            detail
        ),
    }
}
