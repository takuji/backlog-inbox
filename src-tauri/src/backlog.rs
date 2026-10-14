use crate::i18n::{self, Lang};
use backlog_api_rs::{BacklogClient, Error as ApiError};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// このアプリが名乗る `User-Agent` の製品トークン。
///
/// `backlog-api-rs` 側のトークンはこの後ろに付く。移行前の独自クライアントも
/// 同じ名前を名乗っていたが、移行時に黙って失われていた。
const USER_AGENT: &str = concat!("backlog-inbox/", env!("CARGO_PKG_VERSION"));

/// 画像1枚の上限。アイコンとロゴはいずれも小さい。
const IMAGE_LIMIT_BYTES: u64 = 5 * 1024 * 1024;

/// 1リクエストあたりの上限。移行前の独自クライアントと同じ値
/// (`.timeout(Duration::from_secs(30))`)。
///
/// ライブラリ側は「チャンクが届かない時間」の上限(60秒)しか持たない。
/// 大きな添付ファイルの正常な転送を、届き続けている限り妨げないための
/// 設計であり、全体の所要時間には上限が無い。このアプリが実際に扱うのは
/// 小さなJSONと画像だけなので、呼び出し側でこのアプリ独自の全体上限を
/// かけ直す。`refresh.rs` のポーラーは各リフレッシュを順番に待つだけで
/// 自前のタイムアウトを持たないため、ここで止めないとポーリング全体が
/// 無言のまま止まってしまう。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// 設定のホストからベースURLを作る。
///
/// **APIの base_url と DTO に載せるリンクURLの両方がこれを使う。**
/// 2か所で別々に組み立てると、片方だけ直したときに食い違う。
///
/// スキームが既に付いていれば尊重する。テストのモックサーバーは http で
/// 立つため、これが無いと到達できない。設定画面の `normalize_host`
/// (`settings.rs`)が `Settings` を作る前にスキームを剥がすので、利用者が
/// 設定にスキーム付きの値を貼り付けても実害は無いはずだが、ここでも
/// スキームを尊重しておくことで、その前提が崩れたときの保険にもなる。
pub(crate) fn base_url(host: &str) -> String {
    let host = host.trim_end_matches('/');
    if host.starts_with("http://") || host.starts_with("https://") {
        host.to_string()
    } else {
        format!("https://{host}")
    }
}

/// 担当している未完了課題の絞り込み。
///
/// 「完了」(全プロジェクト共通で id 4)以外、と否定で指定する。標準ステータス
/// 1:未対応 2:処理中 3:処理済み を列挙していたが、プロジェクトごとに追加された
/// カスタムステータスは id がそれぞれ別に割り当てられるため、未完了なのに
/// 一覧・件数の両方から漏れていた。
fn my_issue_params(me: i64) -> backlog_api_rs::api::GetIssuesParams {
    backlog_api_rs::api::GetIssuesParams::new()
        .assignee_ids([me as u64])
        .not_status_ids([4])
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub space_host: String,
    pub api_key: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct User {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Space {
    pub name: String,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDto {
    pub id: i64,
    pub already_read: bool,
    /// 通知に紐づくリソース(課題・コメント等)を閲覧済みか
    pub resource_already_read: bool,
    pub reason: i64,
    pub sender_id: Option<i64>,
    pub sender_name: String,
    pub project_name: String,
    pub title: String,
    pub comment_snippet: Option<String>,
    pub url: String,
    pub created: String,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct IssueDto {
    pub id: i64,
    pub issue_key: String,
    pub summary: String,
    pub due_date: Option<String>,
    pub status_name: String,
    pub url: String,
    pub creator_id: Option<i64>,
    pub creator_name: String,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WatchingDto {
    pub id: i64,
    pub updated: bool,
    pub issue_key: String,
    pub summary: String,
    pub status_name: String,
    pub url: String,
    pub last_content_updated: Option<String>,
    pub creator_id: Option<i64>,
    pub creator_name: String,
}

#[derive(Clone, Default, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub space_name: String,
    pub notifications: Vec<NotificationDto>,
    /// さらに古い通知を取得できる可能性があるか
    pub has_more_notifications: bool,
    pub unread_count: i64,
    pub issues: Vec<IssueDto>,
    pub issue_count: i64,
    pub watchings: Vec<WatchingDto>,
    /// 直近の取得で読めなかった通知の件数。累計ではない。
    pub skipped_notifications: usize,
}

/// 1回のリクエストで取得する通知件数
pub const NOTIF_PAGE: usize = 50;

/// 通知1ページ分。読めなかった件数を一緒に運ぶ。
///
/// タプルにしないのは、呼び出し側で要素を取り違えても型で気づけず、
/// 読み手にも意味が伝わらないため。
#[derive(Clone, Debug)]
pub struct NotificationPage {
    pub items: Vec<NotificationDto>,
    /// この取得で読めなかった件数。**累計ではない。**
    pub skipped: usize,
}

/// 実APIのタイムスタンプ形式(`2026-07-30T07:55:07Z`)で文字列化する。
///
/// `to_rfc3339()` は `+00:00` を付けるため**使ってはいけない**。
/// フロントエンドに渡る形が変わってしまう。
fn iso_z(t: chrono::DateTime<chrono::Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// 通知に埋め込まれたコメントから抜粋の元テキストを選ぶ。
///
/// `NotificationComment` は `comment` / `pull_request_comment` /
/// `document_comment` / `document_comment_reply` の4フィールド共通の型
/// (`backlog-api-rs` の `models::NotificationComment`)。ドキュメントの
/// コメントは `content`(書式付き)と `plain` の両方を持ち、抜粋には
/// `plain` を優先すべきとライブラリのコメントに明記されている
/// (`backlog-api-rs/src/models/notification.rs`)。課題・プルリクエストの
/// コメントは `plain` を持たない(`None`)ため、`content` にフォールバックする
/// (現状の挙動のまま)。
fn comment_text(c: &backlog_api_rs::models::NotificationComment) -> Option<&str> {
    c.plain.as_deref().or(c.content.as_deref())
}

/// コメント本文から抜粋を作る(trim → 改行を空白に → 80文字で切る)。
fn snippet(content: &str) -> Option<String> {
    let t = content.trim().replace('\n', " ");
    if t.is_empty() {
        return None;
    }
    let s: String = t.chars().take(80).collect();
    Some(if t.chars().count() > 80 {
        format!("{s}…")
    } else {
        s
    })
}

/// 画像のマジックバイトから MIME を判定する。
///
/// スペースロゴは Content-Type が application/octet-stream で返ることが
/// あるため、ヘッダーを信用せず中身から判定する。**これは実APIで観測した
/// 事実ではなく**、移行前からアプリに残っていたコメントを根拠にしている
/// (設計書 決定事項6)。
fn sniff_image_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else {
        "image/png"
    }
}

/// `base` は `base_url()` が返すスキーム付きのベースURL。
fn to_notification(base: &str, n: &backlog_api_rs::models::Notification) -> NotificationDto {
    let title = if let Some(issue) = n.issue.as_ref() {
        format!("{} {}", issue.issue_key, issue.summary)
    } else if let Some(number) = n.pull_request.as_ref().and_then(|pr| pr.number) {
        // `number` が無い(null)プルリクエスト通知は形が不完全なので、この分岐に
        // 入れず下のプロジェクト名フォールバックへ落とす(「PR #0」を防ぐ)。
        let summary = n
            .pull_request
            .as_ref()
            .and_then(|pr| pr.summary.clone())
            .unwrap_or_default();
        format!("PR #{number} {summary}")
    } else if let Some(title) = n.document.as_ref().and_then(|d| d.title.clone()) {
        title
    } else {
        n.project
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Backlog".to_string())
    };

    let comment_snippet = n
        .comment
        .as_ref()
        .and_then(comment_text)
        .and_then(snippet)
        .or_else(|| {
            n.pull_request_comment
                .as_ref()
                .and_then(comment_text)
                .and_then(snippet)
        })
        .or_else(|| {
            n.document_comment
                .as_ref()
                .and_then(comment_text)
                .and_then(snippet)
        })
        .or_else(|| {
            n.document_comment_reply
                .as_ref()
                .and_then(comment_text)
                .and_then(snippet)
        });

    NotificationDto {
        id: n.id as i64,
        already_read: n.already_read,
        resource_already_read: n.resource_already_read,
        reason: n.reason as i64,
        sender_id: n.sender.as_ref().map(|u| u.id as i64),
        sender_name: n
            .sender
            .as_ref()
            .map(|u| u.name.clone())
            .unwrap_or_default(),
        project_name: n
            .project
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default(),
        title,
        comment_snippet,
        // サーバー側で通知内容に応じた画面へリダイレクトされるエンドポイント
        url: format!("{base}/globalbar/notifications/redirect/{}", n.id),
        created: iso_z(n.created),
    }
}

/// 課題の詳細画面のURL。`to_issue` と `to_watching` の両方が使う。
fn issue_url(base: &str, issue_key: &str) -> String {
    format!("{base}/view/{issue_key}")
}

/// 課題の最終更新日時。一度も更新されていない課題は登録日時を使う。
fn last_touched(i: &backlog_api_rs::models::Issue) -> chrono::DateTime<chrono::Utc> {
    i.updated.unwrap_or(i.created)
}

fn to_issue(base: &str, i: &backlog_api_rs::models::Issue) -> IssueDto {
    IssueDto {
        id: i.id as i64,
        url: issue_url(base, &i.issue_key),
        issue_key: i.issue_key.clone(),
        summary: i.summary.clone(),
        // 実APIは 2024-08-31T00:00:00Z を返すので日付部分だけにする
        due_date: i.due_date.map(|d| d.format("%Y-%m-%d").to_string()),
        status_name: i.status.name.clone(),
        creator_id: Some(i.created_user.id as i64),
        creator_name: i.created_user.name.clone(),
    }
}

fn to_watching(base: &str, w: &backlog_api_rs::models::Watching) -> Option<WatchingDto> {
    let issue = w.issue.as_ref()?;
    Some(WatchingDto {
        id: w.id as i64,
        updated: !w.resource_already_read,
        url: issue_url(base, &issue.issue_key),
        issue_key: issue.issue_key.clone(),
        summary: issue.summary.clone(),
        status_name: issue.status.name.clone(),
        last_content_updated: w.last_content_updated.map(iso_z),
        creator_id: Some(issue.created_user.id as i64),
        creator_name: issue.created_user.name.clone(),
    })
}

#[derive(Clone)]
pub struct Client {
    /// 設定ごとに構築した `BacklogClient`。
    ///
    /// `BacklogClient` は base_url と api_key を握って構築されるため、
    /// 呼び出しのたびに作ると接続プールが効かない(1回のリフレッシュで
    /// 6コール、それぞれTLSハンドシェイクからやり直しになる)。
    /// 設定は実行時に変わりうるので、変わったときだけ作り直す。
    cached: Arc<Mutex<Option<(Settings, BacklogClient)>>>,
    lang: Arc<Mutex<Lang>>,
}

impl Client {
    pub fn new(lang: Arc<Mutex<Lang>>) -> Self {
        Self {
            cached: Arc::new(Mutex::new(None)),
            lang,
        }
    }

    fn lang(&self) -> Lang {
        *self.lang.lock().unwrap()
    }

    /// 設定に対応する `BacklogClient` を返す。設定が変わっていれば作り直す。
    ///
    /// `BacklogClient` は `Clone` なので複製して返し、**ロックを `.await` を
    /// またいで持たない**。
    fn api(&self, st: &Settings) -> Result<BacklogClient, String> {
        let mut guard = self.cached.lock().unwrap();
        if let Some((cached_settings, client)) = guard.as_ref() {
            if cached_settings == st {
                return Ok(client.clone());
            }
        }
        // このアプリの名前を名乗る。ライブラリのトークンは後ろに残るので、
        // サーバーのログから「どのアプリか」と「クライアントのどの版か」の
        // 両方が分かる。組み込んだMCPサーバーは別のトークンを名乗るので、
        // 定期取得とツール呼び出しも区別できる。
        let client = BacklogClient::builder(&base_url(&st.space_host), &st.api_key)
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| i18n::err_connect(&e.to_string(), self.lang()))?;
        *guard = Some((st.clone(), client.clone()));
        Ok(client)
    }

    /// ライブラリのエラーをユーザー向けの文言に変換する。
    ///
    /// ライブラリの `Display` は `without_url()` 済みなのでAPIキーを含まない。
    fn map_error(&self, e: ApiError) -> String {
        use backlog_api_rs::ErrorCode;
        let lang = self.lang();
        match e {
            ApiError::Api { status, ref errors } => {
                let has = |c: ErrorCode| errors.iter().any(|x| x.code == c);
                if status == 401 || has(ErrorCode::Authentication) {
                    i18n::err_auth(lang)
                } else if status == 404 || has(ErrorCode::NoResource) {
                    i18n::err_not_found(lang)
                } else {
                    i18n::err_api(status, lang)
                }
            }
            // 非2xxのボディが `{"errors":[…]}` として解析できなかった場合。
            // ステータスだけは分かっているので、401/404はここでも
            // 認証・リソース不在の文言にする。プロキシやCDNがJSONでない
            // ボディ(HTMLのエラーページ等)を挟んで返すケースがこれに当たり、
            // ここを無条件に `err_api` へ流すと「APIキーを確認してください」と
            // 案内すべき401が汎用のエラー番号表示に化けてしまう。
            ApiError::UnexpectedResponse { status, .. } => match status {
                401 => i18n::err_auth(lang),
                404 => i18n::err_not_found(lang),
                other => i18n::err_api(other, lang),
            },
            // 通信エラーとデコードエラーはどちらも `Http` にまとまるので分ける。
            // 取り違えると、解析失敗が「接続に失敗しました」と表示される。
            ApiError::Http(ref inner) if inner.is_decode() => i18n::err_parse(&e.to_string(), lang),
            other => i18n::err_connect(&other.to_string(), lang),
        }
    }

    /// 1リクエストに `REQUEST_TIMEOUT` の上限をかけて待つ。
    ///
    /// ライブラリは「チャンクが届かない時間」の上限しか持たないため、
    /// 応答が来ないまま繋がり続けるリクエストを外側で断ち切る必要がある
    /// (`REQUEST_TIMEOUT` のコメント参照)。
    async fn deadline<T>(
        &self,
        fut: impl Future<Output = Result<T, ApiError>>,
    ) -> Result<T, String> {
        match tokio::time::timeout(REQUEST_TIMEOUT, fut).await {
            Ok(result) => result.map_err(|e| self.map_error(e)),
            Err(_) => {
                let lang = self.lang();
                let secs = REQUEST_TIMEOUT.as_secs();
                let detail = match lang {
                    Lang::Ja => format!("リクエストが{secs}秒以内に完了しませんでした"),
                    Lang::En => format!("The request did not complete within {secs} seconds"),
                };
                Err(i18n::err_connect(&detail, lang))
            }
        }
    }

    pub async fn myself(&self, st: &Settings) -> Result<User, String> {
        let api = self.api(st)?;
        let me = self.deadline(api.users().myself()).await?;
        Ok(User {
            id: me.id as i64,
            name: me.name,
        })
    }

    pub async fn space(&self, st: &Settings) -> Result<Space, String> {
        let api = self.api(st)?;
        let space = self.deadline(api.space().info()).await?;
        Ok(Space { name: space.name })
    }

    /// スペースのロゴ画像を (バイト列, MIME) で取得する。
    ///
    /// リクエストと本文読み込みの2回の awaited 呼び出しをまとめて1つの
    /// `REQUEST_TIMEOUT` で括る。移行前の独自クライアントは reqwest の
    /// `.timeout()` で1リクエスト分をまとめて計っていたため、そこに合わせる
    /// (2回に分けて budget を与えると、合計の許容時間が実質2倍になり
    /// 移行前より緩くなってしまう)。
    pub async fn space_logo(&self, st: &Settings) -> Result<(Vec<u8>, String), String> {
        let api = self.api(st)?;
        let bytes = self
            .deadline(async move {
                let download = api.space().image().await?;
                download.read_to_vec(Some(IMAGE_LIMIT_BYTES)).await
            })
            .await?;
        // Content-Type が application/octet-stream で返ることがあるため中身から判定する
        let mime = sniff_image_mime(&bytes).to_string();
        Ok((bytes, mime))
    }

    /// ユーザーのアイコン画像を (バイト列, Content-Type) で取得する。
    ///
    /// このエンドポイントはアイコンの実体がNulabアカウント側にある場合、
    /// 外部URLへ302リダイレクトする。ライブラリは追従するが、APIキーが
    /// リダイレクト先へ漏れないよう `Referer` を切ってある。
    ///
    /// `space_logo` と同様、リクエストと本文読み込みをまとめて1つの
    /// `REQUEST_TIMEOUT` で括る。
    pub async fn user_icon(
        &self,
        st: &Settings,
        user_id: i64,
    ) -> Result<(Vec<u8>, String), String> {
        let api = self.api(st)?;
        let (mime, bytes) = self
            .deadline(async move {
                let download = api.users().icon(user_id as u64).await?;
                // 現状の挙動を維持する: image/* のみ採用し、それ以外は image/png とみなす
                let mime = download
                    .content_type
                    .as_deref()
                    .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
                    .filter(|s| s.starts_with("image/"))
                    .unwrap_or_else(|| "image/png".to_string());
                let bytes = download.read_to_vec(Some(IMAGE_LIMIT_BYTES)).await?;
                Ok((mime, bytes))
            })
            .await?;
        Ok((bytes, mime))
    }

    pub async fn unread_count(&self, st: &Settings) -> Result<i64, String> {
        let api = self.api(st)?;
        let count = self.deadline(api.notifications().unread_count()).await?;
        Ok(count as i64)
    }

    /// 担当している未完了課題の件数。一覧を取得すべきかの判定に使う。
    pub async fn issues_count(&self, st: &Settings, me: i64) -> Result<i64, String> {
        let api = self.api(st)?;
        let count = self
            .deadline(api.issues().count(my_issue_params(me)))
            .await?;
        Ok(count as i64)
    }

    /// 更新のある(未読の)ウォッチ数。ウォッチ一覧を取得すべきかの判定に使う。
    pub async fn watching_count(&self, st: &Settings, me: i64) -> Result<i64, String> {
        let api = self.api(st)?;
        let count = self
            .deadline(api.watchings().unread_count_for_user(me as u64))
            .await?;
        Ok(count as i64)
    }

    pub async fn mark_read(&self, st: &Settings, id: i64) -> Result<(), String> {
        let api = self.api(st)?;
        self.deadline(api.notifications().mark_as_read(id as u64))
            .await
    }

    /// 「未読の通知がある」という集計状態をリセットする。
    ///
    /// **個々の通知は既読にならない。** エンドポイントの綴りは
    /// `POST /notifications/markAsRead` だが、公式表題は
    /// "Reset Unread Notification Count" であり、カウンタのリセットである。
    /// メソッド名は呼び出し元への波及を避けて据え置いている。
    pub async fn mark_all_read(&self, st: &Settings) -> Result<(), String> {
        let api = self.api(st)?;
        self.deadline(api.notifications().reset_unread_count())
            .await
            .map(|_| ())
    }

    /// 通知を新しい順に取得する。max_id を指定すると、そのIDより古い通知を返す。
    pub async fn notifications(
        &self,
        st: &Settings,
        max_id: Option<i64>,
    ) -> Result<NotificationPage, String> {
        let api = self.api(st)?;
        let mut params =
            backlog_api_rs::api::GetNotificationsParams::new().count(NOTIF_PAGE as u64);
        if let Some(max_id) = max_id {
            params = params.max_id(max_id as u64);
        }
        let list = self.deadline(api.notifications().list(params)).await?;
        let base = base_url(&st.space_host);
        Ok(NotificationPage {
            items: list
                .items
                .iter()
                .map(|n| to_notification(&base, n))
                .collect(),
            skipped: list.skipped.len(),
        })
    }

    /// 担当している未完了課題の一覧。
    ///
    /// 期限日の昇順、期限なしは末尾。期限が同じ(期限なしどうしも含む)場合は
    /// 最終更新の新しい順にする。`updated` は一度も更新されていない課題では
    /// 空になるので、その場合は登録日時を最終更新として扱う。
    pub async fn my_issues(&self, st: &Settings, me: i64) -> Result<Vec<IssueDto>, String> {
        let api = self.api(st)?;
        let params = my_issue_params(me)
            .sort("dueDate")
            .order(backlog_api_rs::api::Order::Asc)
            .count(100);
        let mut issues = self.deadline(api.issues().list(params)).await?;
        // 期限なしを末尾に寄せ、同じ期限の中の順序を決める
        // (APIのソートだけでは保証されないため並べ直す)。DTO へ変換する前の
        // モデルを並べるのは、判断に使う日時を DTO に持ち出さずに済むため。
        issues.sort_by(|a, b| {
            let by_due = match (&a.due_date, &b.due_date) {
                // 実APIは時刻を 00:00:00Z で返すが、同じ日を確実に同着として
                // 扱うため日付で比べる。
                (Some(x), Some(y)) => x.date_naive().cmp(&y.date_naive()),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };
            // 新しい順なので b と a を入れ替えて比べる。
            by_due.then_with(|| last_touched(b).cmp(&last_touched(a)))
        });
        let base = base_url(&st.space_host);
        Ok(issues.iter().map(|i| to_issue(&base, i)).collect())
    }

    pub async fn watchings(&self, st: &Settings, me: i64) -> Result<Vec<WatchingDto>, String> {
        let api = self.api(st)?;
        let params = backlog_api_rs::api::GetWatchingsParams::new().count(100);
        let list = self
            .deadline(api.watchings().list_for_user(me as u64, params))
            .await?;
        let base = base_url(&st.space_host);
        Ok(list.iter().filter_map(|w| to_watching(&base, w)).collect())
    }
}

/// テスト用の最小限の `NotificationDto` を作る。
///
/// `state.rs` と `refresh.rs` のテストがそれぞれ同じ内容を複製していたので、
/// ここに1つだけ置く(finding 6)。
#[cfg(test)]
pub(crate) fn test_notification_dto(id: i64) -> NotificationDto {
    NotificationDto {
        id,
        already_read: false,
        resource_already_read: false,
        reason: 2,
        sender_id: None,
        sender_name: String::new(),
        project_name: String::new(),
        title: format!("t{id}"),
        comment_snippet: None,
        url: String::new(),
        created: "2026-07-30T07:55:07Z".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// テスト用のクライアントと設定を作る。
    fn client_and_settings(server: &MockServer, lang: Lang) -> (Client, Settings) {
        let client = Client::new(Arc::new(Mutex::new(lang)));
        // モックサーバーは http で立つ。`base_url()` がスキーム付きの host を
        // 尊重するので、`server.uri()` をそのまま入れられる
        let settings = Settings {
            space_host: server.uri(),
            api_key: "KEY".to_string(),
        };
        (client, settings)
    }

    /// テスト用のプロジェクトJSONオブジェクト。
    ///
    /// フィールドは実APIの応答をそのまま書き起こしたもので、どのテストでも
    /// 同じ値を使う(`project_name` は `"プロジェクト名"` で固定)。project.id は
    /// `converts_an_issue_notification` の sender.id(77)と重複しない値(2)。
    fn project_json() -> String {
        r#"{ "id": 2, "projectKey": "PROJ", "name": "プロジェクト名",
             "chartEnabled": true, "subtaskingEnabled": true,
             "projectLeaderCanEditProjectLeader": false,
             "useWiki": true, "useFileSharing": true,
             "useWikiTreeView": true, "useOriginalImageSizeAtWiki": false,
             "textFormattingRule": "backlog", "archived": false,
             "displayOrder": 0, "useDevAttributes": true }"#
            .to_string()
    }

    /// テスト用の課題JSONオブジェクト。
    ///
    /// `to_issue` / `to_watching` が実際に読むフィールド(id, issueKey, summary,
    /// dueDate, status.name, createdUser)だけをパラメータにする。`issueType` の
    /// 色や `displayOrder` など変換で読まれない部分は固定値にする。
    fn issue_json(
        id: i64,
        key: &str,
        summary: &str,
        status_name: &str,
        creator_id: i64,
        due_date: Option<&str>,
    ) -> String {
        issue_json_with_times(
            id,
            key,
            summary,
            status_name,
            creator_id,
            due_date,
            "2013-04-23T07:38:59Z",
            None,
        )
    }

    /// 並び替えのテスト用に登録日時と最終更新日時も指定できる `issue_json`。
    ///
    /// 位置引数がこれ以上増えるのを避けるため、既定値でよい呼び出し元は
    /// `issue_json` から委譲する。
    #[allow(clippy::too_many_arguments)]
    fn issue_json_with_times(
        id: i64,
        key: &str,
        summary: &str,
        status_name: &str,
        creator_id: i64,
        due_date: Option<&str>,
        created: &str,
        updated: Option<&str>,
    ) -> String {
        let due_date_field = due_date
            .map(|d| format!(r#""dueDate": "{d}","#))
            .unwrap_or_default();
        let updated_field = updated
            .map(|d| format!(r#""updated": "{d}","#))
            .unwrap_or_default();
        format!(
            r##"{{ "id": {id}, "projectId": 2, "issueKey": "{key}", "keyId": {id},
                   "issueType": {{ "id": 7, "projectId": 2, "name": "バグ",
                                  "color": "#990000", "displayOrder": 0 }},
                   "summary": "{summary}", {due_date_field} {updated_field}
                   "status": {{ "id": 1, "projectId": 2, "name": "{status_name}",
                               "color": "#ed8077", "displayOrder": 1000 }},
                   "createdUser": {{ "id": {creator_id}, "userId": "admin",
                                    "name": "作成者", "roleType": 1 }},
                   "created": "{created}" }}"##
        )
    }

    /// リクエストに `backlog-inbox/<version> backlog-api-rs/<version>` が
    /// 付くこと。
    ///
    /// 移行前の独自クライアントも同じ名前を名乗っていたが、移行時に黙って
    /// 失われていた。ライブラリの版は下流からコンパイル時に知れないので、
    /// 自分のトークンと順序だけを厳密に固定し、後続は形で見る。
    #[tokio::test]
    async fn requests_carry_both_product_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .and(wiremock::matchers::header_regex(
                "user-agent",
                &format!(
                    r"^backlog-inbox/{} backlog-api-rs/\d+\.\d+\.\d+$",
                    env!("CARGO_PKG_VERSION").replace('.', r"\.")
                ),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"id":1,"userId":"admin","name":"わたし","roleType":1}"#,
                "application/json",
            ))
            .expect(1)
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(client.myself(&st).await.unwrap().name, "わたし");
    }

    /// 401 は認証エラーの文言になること(接続エラーやAPIエラーではない)。
    #[tokio::test]
    async fn maps_401_to_the_auth_message() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(ResponseTemplate::new(401).set_body_raw(
                r#"{"errors":[{"message":"Authentication failure","code":11}]}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let err = client.myself(&st).await.unwrap_err();
        assert_eq!(err, i18n::err_auth(Lang::Ja));
    }

    /// 404 はリソース不在の文言になること。
    #[tokio::test]
    async fn maps_404_to_the_not_found_message() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(ResponseTemplate::new(404).set_body_raw(
                r#"{"errors":[{"message":"No such resource","code":6}]}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(
            client.myself(&st).await.unwrap_err(),
            i18n::err_not_found(Lang::Ja)
        );
    }

    /// その他のステータスは汎用APIエラーの文言(ステータス番号入り)になること。
    #[tokio::test]
    async fn maps_other_statuses_to_the_generic_api_message() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(ResponseTemplate::new(500).set_body_raw("boom", "text/plain"))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(
            client.myself(&st).await.unwrap_err(),
            i18n::err_api(500, Lang::Ja)
        );
    }

    /// 401だがボディが `{"errors":[…]}` として解析できない(非JSON)場合でも、
    /// 認証エラーの文言になること。
    ///
    /// ライブラリは非2xxのボディがこの形で解析できないと `UnexpectedResponse`
    /// を返す(`Api` にはならない)。ここを無条件に `err_api` へ流すと、
    /// プロキシやCDNがHTML等の非JSONボディを挟んで401を返すケースで
    /// 「APIキーを確認してください」の案内が出なくなる(finding 2)。
    #[tokio::test]
    async fn maps_a_401_with_a_non_json_body_to_the_auth_message() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(
                ResponseTemplate::new(401).set_body_raw("<html>proxy error</html>", "text/plain"),
            )
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(
            client.myself(&st).await.unwrap_err(),
            i18n::err_auth(Lang::Ja)
        );
    }

    /// 404だがボディが非JSONの場合でも、リソース不在の文言になること
    /// (上のテストの404版。finding 2)。
    #[tokio::test]
    async fn maps_a_404_with_a_non_json_body_to_the_not_found_message() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(
                ResponseTemplate::new(404).set_body_raw("<html>not found</html>", "text/plain"),
            )
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(
            client.myself(&st).await.unwrap_err(),
            i18n::err_not_found(Lang::Ja)
        );
    }

    /// **壊れたJSONは「解析できません」であって「接続に失敗」ではないこと。**
    ///
    /// ライブラリはどちらも `Error::Http` にまとめるため、`is_decode()` で
    /// 分けないと取り違える。取り違えるとユーザーに誤った原因が表示される。
    #[tokio::test]
    async fn maps_a_broken_body_to_the_parse_message_not_connect() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("{ not json", "application/json"))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let err = client.myself(&st).await.unwrap_err();
        assert!(
            err.starts_with("Backlog APIの応答を解析できませんでした"),
            "解析エラーの文言になっていない: {err}"
        );
        assert!(
            !err.starts_with("Backlogへの接続に失敗しました"),
            "接続エラーと取り違えている: {err}"
        );
    }

    /// 401以外のステータスでも認証エラーコード(11)が含まれていれば認証エラーの
    /// 文言になること。
    ///
    /// ステータスだけで判定すると、Backlogがステータス以外(例:403)で
    /// 認証エラーコードを返すケースを見落とす。この変異(ステータスのみで
    /// 判定)は他のテストでは検出できなかったため追加した。
    #[tokio::test]
    async fn maps_authentication_error_code_even_with_a_non_401_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(ResponseTemplate::new(403).set_body_raw(
                r#"{"errors":[{"message":"Authentication failure","code":11}]}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(
            client.myself(&st).await.unwrap_err(),
            i18n::err_auth(Lang::Ja)
        );
    }

    /// 404以外のステータスでもリソース不在エラーコード(6)が含まれていれば
    /// リソース不在の文言になること。
    ///
    /// ステータスだけで判定すると、Backlogがステータス以外でリソース不在
    /// コードを返すケースを見落とす。`maps_authentication_error_code_even_with_a_non_401_status`
    /// と対になるテスト(このコードパスは非対称だった: 認証側だけコード判定が
    /// テストされ、こちらは404というステータスの一致だけで満たされてしまっていた)。
    #[tokio::test]
    async fn maps_no_resource_error_code_even_with_a_non_404_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(ResponseTemplate::new(400).set_body_raw(
                r#"{"errors":[{"message":"No such resource","code":6}]}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(
            client.myself(&st).await.unwrap_err(),
            i18n::err_not_found(Lang::Ja)
        );
    }

    /// 英語でも同じ分岐になること。
    ///
    /// 文言の切り替えは `lang` に依存するので、日本語だけ通っても英語が
    /// 壊れている可能性がある。
    #[tokio::test]
    async fn maps_errors_in_english_too() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(ResponseTemplate::new(401).set_body_raw(
                r#"{"errors":[{"message":"Authentication failure","code":11}]}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::En);
        assert_eq!(
            client.myself(&st).await.unwrap_err(),
            i18n::err_auth(Lang::En)
        );
    }

    /// **エラーメッセージにAPIキーが含まれないこと。**
    ///
    /// ライブラリの `Error` は `without_url()` 済みだが、アプリ側で
    /// URLを足したりしていないことをここで固定する。デコードエラー
    /// (`err_parse`)経路に加えて、接続エラー(`err_connect`)経路も確認する。
    /// `err_connect` は `other.to_string()` をそのままメッセージに埋め込むため、
    /// 接続先URLを運びやすい。
    #[tokio::test]
    async fn error_messages_never_contain_the_api_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("{ not json", "application/json"))
            .mount(&server)
            .await;

        let client = Client::new(Arc::new(Mutex::new(Lang::Ja)));
        let st = Settings {
            space_host: server.uri(),
            api_key: "SUPER_SECRET_KEY".to_string(),
        };
        let err = client.myself(&st).await.unwrap_err();
        assert!(
            !err.contains("SUPER_SECRET_KEY"),
            "APIキーが漏れている: {err}"
        );
        assert!(
            !err.contains(&st.space_host),
            "ホスト名(URL)が漏れている: {err}"
        );

        // 接続エラー経路: 何も listen していないポートに向ける。
        let dead = Settings {
            space_host: "127.0.0.1:1".to_string(),
            api_key: "SUPER_SECRET_KEY".to_string(),
        };
        let err = client.myself(&dead).await.unwrap_err();
        assert!(
            !err.contains("SUPER_SECRET_KEY"),
            "APIキーが漏れている(接続エラー): {err}"
        );
        assert!(
            !err.contains(&dead.space_host),
            "ホスト名(URL)が漏れている(接続エラー): {err}"
        );
    }

    /// `deadline` のタイムアウト包装が実在し、ハングせずこの文言のエラーを
    /// 返すこと(finding 3)。
    ///
    /// **このテストが証明していないこと:** `set_delay` を外しても
    /// このテストは通る(確認済み)。`start_paused = true` の下では、
    /// 進行可能なタスクが無くなると tokio が時計を次のタイマー期限まで
    /// 自動で進める(auto-advance)ため、`set_delay` の長さに関わらず
    /// `tokio::time::timeout` 自身のタイマーで `REQUEST_TIMEOUT` 経過まで
    /// 時計が進んでしまう。つまりこのテストは「応答が
    /// `REQUEST_TIMEOUT` より遅れると…」という遅延の大小を検証しては
    /// いない。証明しているのは、(1) `deadline` のタイムアウト包装が
    /// 実際に効いていること、(2) タイムアウト時にこの specific な文言が
    /// 返ること、(3) 応答を待ち続けてハングしないこと、の3点のみ。
    /// `set_delay` はモックが即座に応答してしまわないようにする保険として
    /// 残す(無くても現状は通るが、`ResponseTemplate` のデフォルト遅延が
    /// 将来変わった場合の保険にもなる)。
    #[tokio::test(start_paused = true)]
    async fn myself_times_out_when_the_response_is_slower_than_the_budget() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(
                        r#"{"id":1,"userId":"admin","name":"わたし","roleType":1}"#,
                        "application/json",
                    )
                    .set_delay(REQUEST_TIMEOUT + Duration::from_secs(1)),
            )
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let err = client.myself(&st).await.unwrap_err();
        // `REQUEST_TIMEOUT` から文言の秒数を導出する(ハードコードすると
        // 定数を変えたときに文言だけテストから取り残される。finding C)。
        let secs = REQUEST_TIMEOUT.as_secs();
        assert_eq!(
            err,
            i18n::err_connect(
                &format!("リクエストが{secs}秒以内に完了しませんでした"),
                Lang::Ja
            )
        );
    }

    /// 各メソッドが正しいパスを叩き、値が正しく変換されること。
    ///
    /// 1メソッド1モックサーバーにする(パスごとにサーバーを分けると
    /// 「別のメソッドのモックにたまたま当たった」事故が起きない)。
    #[tokio::test]
    async fn myself_returns_the_user() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/myself"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"id":1,"userId":"admin","name":"わたし","roleType":1}"#,
                "application/json",
            ))
            .expect(1)
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let me = client.myself(&st).await.unwrap();
        assert_eq!(me.id, 1);
        assert_eq!(me.name, "わたし");
    }

    #[tokio::test]
    async fn space_returns_the_space_name() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/space"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"spaceKey":"EX","name":"サンプルスペース"}"#,
                "application/json",
            ))
            .expect(1)
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(client.space(&st).await.unwrap().name, "サンプルスペース");
    }

    /// ロゴは Content-Type を信じず中身から判定すること。
    ///
    /// application/octet-stream が返ることがある、という前提での挙動を
    /// 固定する。**この前提は実APIで観測されたものではなく**、移行前から
    /// アプリに残っていたコメントが根拠(設計書 決定事項6)。
    #[tokio::test]
    async fn space_logo_sniffs_the_mime_from_the_bytes() {
        let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0d".to_vec();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/space/image"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(png.clone(), "application/octet-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let (bytes, mime) = client.space_logo(&st).await.unwrap();
        assert_eq!(bytes, png);
        assert_eq!(mime, "image/png");
    }

    /// アイコンは Content-Type の image/* をそのまま使うこと。
    #[tokio::test]
    async fn user_icon_uses_the_image_content_type() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/42/icon"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(b"gif-bytes".to_vec(), "image/gif"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let (bytes, mime) = client.user_icon(&st, 42).await.unwrap();
        assert_eq!(bytes, b"gif-bytes");
        assert_eq!(mime, "image/gif");
    }

    /// image/* 以外の Content-Type は image/png とみなすこと(現状の挙動)。
    #[tokio::test]
    async fn user_icon_falls_back_to_png_for_non_image_types() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/42/icon"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(b"bytes".to_vec(), "application/octet-stream"),
            )
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(client.user_icon(&st, 42).await.unwrap().1, "image/png");
    }

    /// 未読件数は alreadyRead=false のみで数えること。
    ///
    /// resourceAlreadyRead は**送らない**。送ると、Backlogを直接開いて
    /// 対象を読んだ通知が件数から落ちる。
    #[tokio::test]
    async fn unread_count_filters_only_by_already_read() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications/count"))
            .and(wiremock::matchers::query_param("alreadyRead", "false"))
            .and(wiremock::matchers::query_param_is_missing(
                "resourceAlreadyRead",
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(r#"{"count":7}"#, "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(client.unread_count(&st).await.unwrap(), 7);
    }

    /// 課題件数は担当者と「完了以外」で絞ること。
    ///
    /// `notStatusId[]=4` を送り、`statusId[]` は送らないこと。標準ステータスを
    /// 列挙する実装に戻ると、カスタムステータスの課題が件数から落ちる。
    /// `query_param_is_missing` まで見るのは、両方送る中途半端な実装
    /// (`statusId[]` が残ったまま `notStatusId[]` を足す)を弾くため。
    #[tokio::test]
    async fn issues_count_filters_by_assignee_and_excludes_only_closed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/issues/count"))
            .and(wiremock::matchers::query_param("assigneeId[]", "5"))
            .and(wiremock::matchers::query_param("notStatusId[]", "4"))
            .and(wiremock::matchers::query_param_is_missing("statusId[]"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(r#"{"count":3}"#, "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(client.issues_count(&st, 5).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn watching_count_uses_the_user_scoped_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/5/watchings/count"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(r#"{"count":2}"#, "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert_eq!(client.watching_count(&st, 5).await.unwrap(), 2);
    }

    /// 個別の既読化は対象IDのパスに POST すること。
    #[tokio::test]
    async fn mark_read_posts_to_the_notification_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/notifications/123/markAsRead"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        client.mark_read(&st, 123).await.unwrap();
    }

    /// mark_all_read は**カウンタのリセット**であること。
    ///
    /// 個別既読化のパス(`/notifications/{id}/markAsRead`)ではなく
    /// `/notifications/markAsRead` を叩く。取り違えると意味が変わる。
    #[tokio::test]
    async fn mark_all_read_resets_the_counter() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/notifications/markAsRead"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(r#"{"count":0}"#, "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        client.mark_all_read(&st).await.unwrap();
    }

    /// 設定が変わったら BacklogClient を作り直すこと。
    ///
    /// キャッシュしたまま古い設定を使い続けると、設定画面でスペースを
    /// 変えても前のスペースを見続けることになる。ホストが同じでAPIキーだけ
    /// 変えた場合も同様(壊れたAPIキーを打ち直したのに、比較がホストだけを
    /// 見ていると古いクライアントに固定されたままになり、再起動するまで直らない)。
    #[tokio::test]
    async fn rebuilds_the_client_when_settings_change() {
        let first = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/space"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"spaceKey":"A","name":"最初のスペース"}"#,
                "application/json",
            ))
            .mount(&first)
            .await;
        let second = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/space"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"spaceKey":"B","name":"次のスペース"}"#,
                "application/json",
            ))
            .mount(&second)
            .await;

        let (client, st1) = client_and_settings(&first, Lang::Ja);
        assert_eq!(client.space(&st1).await.unwrap().name, "最初のスペース");

        let st2 = Settings {
            space_host: second.uri(),
            api_key: "KEY".to_string(),
        };
        assert_eq!(client.space(&st2).await.unwrap().name, "次のスペース");

        // ホストは同じでAPIキーだけ変えたケース。同一サーバー上で
        // `Backlog-API-Key` ヘッダによりモックを出し分け、新しい鍵が実際に
        // 使われていること(古いクライアントが使い回されていないこと)を確認する。
        let third = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/space"))
            .and(wiremock::matchers::header("Backlog-API-Key", "KEY_A"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"spaceKey":"C","name":"鍵Aのスペース"}"#,
                "application/json",
            ))
            .mount(&third)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v2/space"))
            .and(wiremock::matchers::header("Backlog-API-Key", "KEY_B"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"spaceKey":"D","name":"鍵Bのスペース"}"#,
                "application/json",
            ))
            .mount(&third)
            .await;

        let st3a = Settings {
            space_host: third.uri(),
            api_key: "KEY_A".to_string(),
        };
        let st3b = Settings {
            space_host: third.uri(),
            api_key: "KEY_B".to_string(),
        };
        assert_eq!(client.space(&st3a).await.unwrap().name, "鍵Aのスペース");
        assert_eq!(client.space(&st3b).await.unwrap().name, "鍵Bのスペース");
    }

    /// ベースURLの組み立て。設定にスキームが付いていても壊れないこと。
    #[test]
    fn builds_the_base_url() {
        assert_eq!(
            base_url("example.backlog.com"),
            "https://example.backlog.com"
        );
        assert_eq!(
            base_url("example.backlog.com/"),
            "https://example.backlog.com"
        );
        assert_eq!(
            base_url("https://example.backlog.com"),
            "https://example.backlog.com"
        );
        assert_eq!(base_url("http://127.0.0.1:8080"), "http://127.0.0.1:8080");
    }

    /// 課題への通知が DTO に正しく変換されること。
    ///
    /// アサーションはフィールドごとに行う。値だけを見る書き方では
    /// `title` と `project_name` の取り違えのようなフィールド交換を検出できない。
    #[tokio::test]
    async fn converts_an_issue_notification() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    r#"[{{ "id": 299, "alreadyRead": false, "reason": 2,
                           "resourceAlreadyRead": true,
                           "project": {project},
                           "issue": {issue},
                           "comment": {{ "id": 7007, "content": "コメント本文" }},
                           "sender": {{ "id": 77, "userId": "eguchi", "name": "送信者", "roleType": 2 }},
                           "created": "2026-07-30T07:55:07Z" }}]"#,
                    project = project_json(),
                    // creator_id は 1(project.id=2, sender.id=77 と重複しない値)。
                    issue = issue_json(4531, "PROJ-17", "課題の件名", "未対応", 1, None),
                ),
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.notifications(&st, None).await.unwrap().items;
        assert_eq!(list.len(), 1);
        let n = &list[0];
        assert_eq!(n.id, 299);
        assert!(!n.already_read);
        assert!(n.resource_already_read);
        assert_eq!(n.reason, 2);
        // sender.id はこのfixtureの他のどのidとも重複しない値(77)にしてある。
        // project.id(2)から誤って取得しても偶然一致してしまわないようにするため。
        assert_eq!(n.sender_id, Some(77));
        assert_eq!(n.sender_name, "送信者");
        assert_eq!(n.project_name, "プロジェクト名");
        assert_eq!(n.title, "PROJ-17 課題の件名");
        assert_eq!(n.comment_snippet.as_deref(), Some("コメント本文"));
        // `base_url` を使わず `server.uri()` で組み立てる。`base_url` を両辺で
        // 使うと `base_url` 自体の変異をこのアサーションが検出できなくなる
        // (`builds_the_base_url` が関数単体は別途カバーしている)。
        assert_eq!(
            n.url,
            format!("{}/globalbar/notifications/redirect/299", server.uri())
        );
        // **タイムスタンプの形式を変えないこと。** to_rfc3339() だと +00:00 になる
        assert_eq!(n.created, "2026-07-30T07:55:07Z");
    }

    /// プルリクエストへの通知では title が `PR #{番号} {件名}` になること。
    #[tokio::test]
    async fn converts_a_pull_request_notification_title() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[{ "id": 300, "alreadyRead": false, "reason": 11,
                      "resourceAlreadyRead": false,
                      "pullRequest": { "number": 12, "summary": "PRの件名" },
                      "pullRequestComment": { "id": 5, "content": "PRコメント" },
                      "created": "2026-07-30T07:55:07Z" }]"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.notifications(&st, None).await.unwrap().items;
        assert_eq!(list[0].title, "PR #12 PRの件名");
        // comment が無い場合は pullRequestComment から抜粋を作る
        assert_eq!(list[0].comment_snippet.as_deref(), Some("PRコメント"));
    }

    /// `pullRequest` オブジェクトはあるが `number` が無い(null)場合、
    /// 「PR #0」にはせずプロジェクト名へフォールバックすること。
    ///
    /// `pr.number.unwrap_or_default()` に戻すと `number` が無いだけで
    /// 「PR #0 x」になってしまう(移行前は number が整数であることをキーに
    /// この分岐へ入っていたため、null なら通らなかった)。
    #[tokio::test]
    async fn falls_back_to_project_name_when_pull_request_number_is_missing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    r#"[{{ "id": 304, "alreadyRead": false, "reason": 11,
                           "resourceAlreadyRead": false,
                           "pullRequest": {{ "summary": "x" }},
                           "project": {project},
                           "created": "2026-07-30T07:55:07Z" }}]"#,
                    project = project_json(),
                ),
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.notifications(&st, None).await.unwrap().items;
        assert_eq!(list[0].title, "プロジェクト名");
    }

    /// `comment` と `pullRequestComment` の両方がある場合、`comment` が優先されること。
    ///
    /// 既存のテストはどちらか一方しか持たない fixture だったため、優先順位を
    /// 入れ替える変異(`or_else` の対象を取り違える)を検出できなかった。
    /// 両方を同時に持つ fixture でなければ「優先」を検証したことにならない。
    #[tokio::test]
    async fn prefers_the_issue_comment_over_the_pull_request_comment_when_both_present() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[{ "id": 302, "alreadyRead": false, "reason": 11,
                      "resourceAlreadyRead": false,
                      "comment": { "id": 6, "content": "課題コメント優先" },
                      "pullRequestComment": { "id": 7, "content": "PRコメントは無視される" },
                      "created": "2026-07-30T07:55:07Z" }]"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.notifications(&st, None).await.unwrap().items;
        assert_eq!(list[0].comment_snippet.as_deref(), Some("課題コメント優先"));
    }

    /// 課題もプルリクエストも無い通知では title がプロジェクト名になること。
    #[tokio::test]
    async fn converts_a_project_only_notification_title() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    r#"[{{ "id": 301, "alreadyRead": false, "reason": 6,
                           "resourceAlreadyRead": false,
                           "project": {project},
                           "created": "2026-07-30T07:55:07Z" }}]"#,
                    project = project_json(),
                ),
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.notifications(&st, None).await.unwrap().items;
        assert_eq!(list[0].title, "プロジェクト名");
        assert_eq!(list[0].comment_snippet, None);
    }

    /// ドキュメントへのコメント通知(理由14)では、`comment` ではなく
    /// `documentComment` から抜粋を作り、`title` はドキュメントのタイトルに
    /// なること。**抜粋は `plain` を優先し、`content`(書式付き)は使わない
    /// こと** (`backlog-api-rs` の `models/notification.rs` が明記している
    /// ドキュメントコメントの前提。finding A)。
    ///
    /// Task 4 でフロントエンドに理由14/15/17のラベルを追加したが、
    /// `to_notification` が `document` / `document_comment` /
    /// `document_comment_reply` を読んでいなかったため、正しいラベルが
    /// 付いた行の中身が空になっていた(finding 5)。
    #[tokio::test]
    async fn converts_a_document_comment_notification() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[{ "id": 305, "alreadyRead": false, "reason": 14,
                      "resourceAlreadyRead": false,
                      "document": { "id": "abc", "title": "議事録" },
                      "documentComment": { "id": "cmt-1",
                                            "content": "<p>書式付き本文</p>",
                                            "plain": "プレーン本文" },
                      "created": "2026-07-30T07:55:07Z" }]"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.notifications(&st, None).await.unwrap().items;
        assert_eq!(list[0].title, "議事録");
        assert_eq!(list[0].comment_snippet.as_deref(), Some("プレーン本文"));
    }

    /// ドキュメントのコメントへの返信通知(理由15)では、`documentCommentReply`
    /// から抜粋を作ること。**実APIの形は `plain` だけを持ち `content` を
    /// 持たない**(`backlog-api-rs` のfixtureが `{"id": "rep-1", "plain": "返信本文"}`
    /// であることで裏付けられている)。ここが `content` しか読んでいないと、
    /// この通知の抜粋は常に `None` になる(finding A が指摘した、空行の症状そのもの)。
    #[tokio::test]
    async fn converts_a_document_comment_reply_notification_with_plain_only() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[{ "id": 308, "alreadyRead": false, "reason": 15,
                      "resourceAlreadyRead": false,
                      "document": { "id": "abc", "title": "議事録" },
                      "documentCommentReply": { "id": "rep-1", "plain": "返信本文" },
                      "created": "2026-07-30T07:55:07Z" }]"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.notifications(&st, None).await.unwrap().items;
        assert_eq!(list[0].comment_snippet.as_deref(), Some("返信本文"));
    }

    /// `documentComment` と `documentCommentReply` の両方がある場合、
    /// `documentComment` が優先されること。
    ///
    /// 両方を同時に持つ fixture が無いと、この2つの `or_else` の順序を
    /// 入れ替える変異(finding 5 の「`document_comment` の次に
    /// `document_comment_reply`」という順序)を検出できない
    /// (`.or_else` は片方だけが `Some` の場合は順序を入れ替えても結果が
    /// 変わらないため。mutation testingで未検出だったため追加した)。
    #[tokio::test]
    async fn prefers_the_document_comment_over_the_reply_when_both_present() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[{ "id": 307, "alreadyRead": false, "reason": 14,
                      "resourceAlreadyRead": false,
                      "document": { "id": "abc", "title": "議事録" },
                      "documentComment": { "id": "cmt-1", "content": "コメント優先" },
                      "documentCommentReply": { "id": "rep-1", "content": "返信は無視される" },
                      "created": "2026-07-30T07:55:07Z" }]"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.notifications(&st, None).await.unwrap().items;
        assert_eq!(list[0].comment_snippet.as_deref(), Some("コメント優先"));
    }

    /// ドキュメントでのメンション通知(理由17)は `comment` を持たない。
    /// `title` はドキュメントのタイトルになり、`comment_snippet` は
    /// `None` のままであること(実APIで観測した形。この通知は
    /// `documentComment` / `documentCommentReply` も null)。
    #[tokio::test]
    async fn converts_a_document_mentioned_notification_without_a_comment() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[{ "id": 306, "alreadyRead": false, "reason": 17,
                      "resourceAlreadyRead": false,
                      "comment": null,
                      "document": { "id": "def", "title": "設計メモ" },
                      "documentComment": null, "documentCommentReply": null,
                      "created": "2026-07-30T07:55:07Z" }]"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.notifications(&st, None).await.unwrap().items;
        assert_eq!(list[0].title, "設計メモ");
        assert_eq!(list[0].comment_snippet, None);
    }

    /// 抜粋が80文字で切られ、超過時に `…` が付くこと。境界も固定する。
    #[tokio::test]
    async fn truncates_the_comment_snippet_at_eighty_chars() {
        for (len, expect_ellipsis) in [(79usize, false), (80, false), (81, true)] {
            let content = "あ".repeat(len);
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v2/notifications"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(
                    format!(
                        r#"[{{ "id": 1, "alreadyRead": false, "reason": 2,
                              "resourceAlreadyRead": false,
                              "comment": {{ "id": 1, "content": "{content}" }},
                              "created": "2026-07-30T07:55:07Z" }}]"#
                    ),
                    "application/json",
                ))
                .mount(&server)
                .await;

            let (client, st) = client_and_settings(&server, Lang::Ja);
            let snippet = client.notifications(&st, None).await.unwrap().items[0]
                .comment_snippet
                .clone()
                .unwrap();
            assert_eq!(
                snippet.ends_with('…'),
                expect_ellipsis,
                "len={len} snippet={snippet}"
            );
            let expected_chars = if expect_ellipsis { 81 } else { len };
            assert_eq!(snippet.chars().count(), expected_chars, "len={len}");
        }
    }

    /// `snippet` は前後の空白を trim し、改行を空白1つに畳むこと。
    #[test]
    fn snippet_trims_and_flattens_newlines() {
        assert_eq!(snippet("  a\nb  ").as_deref(), Some("a b"));
    }

    /// **挙動変更の記録:** 移行前は `comment.content` が空白だけの場合、
    /// `snippet` は "trim後に空文字ならNone" ではなく単に空文字を返しており、
    /// `.or_else` で `pull_request_comment` へフォールバックすることは
    /// なかった(空文字はSomeとして扱われ、フォールバックの分岐に入らなかった)。
    /// 移行後は `snippet` が空白だけの内容に対して `None` を返すため、
    /// `comment_snippet` の `.and_then(snippet).or_else(...)` が
    /// `pull_request_comment` 側にフォールバックするようになった。
    /// これは移行前より妥当な挙動だが、未検証・未記載だったため固定する。
    #[tokio::test]
    async fn falls_back_to_pull_request_comment_when_issue_comment_is_blank() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[{ "id": 303, "alreadyRead": false, "reason": 11,
                      "resourceAlreadyRead": false,
                      "comment": { "id": 8, "content": "   " },
                      "pullRequestComment": { "id": 9, "content": "PRコメントへフォールバック" },
                      "created": "2026-07-30T07:55:07Z" }]"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.notifications(&st, None).await.unwrap().items;
        assert_eq!(
            list[0].comment_snippet.as_deref(),
            Some("PRコメントへフォールバック")
        );
    }

    /// `max_id` がクエリに乗り、件数が 50 で送られること。
    #[tokio::test]
    async fn notifications_sends_max_id_and_page_size() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .and(wiremock::matchers::query_param("maxId", "500"))
            .and(wiremock::matchers::query_param("count", "50"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("[]", "application/json"))
            .expect(1)
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        client.notifications(&st, Some(500)).await.unwrap();
    }

    /// 担当課題が DTO に正しく変換されること。
    #[tokio::test]
    async fn converts_my_issues() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/issues"))
            // my_issues が課題一覧の絞り込みに載せる独自のパラメータ。
            // これらを固定しないと、削除しても素通りしてしまう(mutation testで確認済み)。
            .and(wiremock::matchers::query_param("sort", "dueDate"))
            .and(wiremock::matchers::query_param("order", "asc"))
            .and(wiremock::matchers::query_param("count", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    "[{}]",
                    issue_json(
                        4531,
                        "PROJ-17",
                        "課題の件名",
                        "未対応",
                        9,
                        Some("2024-08-31T00:00:00Z"),
                    )
                ),
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.my_issues(&st, 1).await.unwrap();
        let i = &list[0];
        assert_eq!(i.id, 4531);
        assert_eq!(i.issue_key, "PROJ-17");
        assert_eq!(i.summary, "課題の件名");
        assert_eq!(i.status_name, "未対応");
        assert_eq!(i.creator_id, Some(9));
        assert_eq!(i.creator_name, "作成者");
        // **日付のみ。** 実APIは 2024-08-31T00:00:00Z を返す
        assert_eq!(i.due_date.as_deref(), Some("2024-08-31"));
        // `base_url` を両辺で使わない(finding 7 参照): `server.uri()` は
        // モックサーバーの実URLそのもの。
        assert_eq!(i.url, format!("{}/view/PROJ-17", server.uri()));
    }

    /// 期限日の並び替え: 昇順で、期限なしは末尾になること。
    ///
    /// `converts_my_issues` は課題1件だけなので `sort_by` の
    /// `(Some(_), None)` / `(None, Some(_))` の分岐が一度も通らない
    /// (この2本の分岐を入れ替えても検出できなかった変異testで確認済み)。
    /// 3件(期限あり2件+期限なし1件)を、期待する出力順と異なる順で
    /// モックから返すことで、ソートが実際に効いていることを固定する。
    #[tokio::test]
    async fn sorts_issues_by_due_date_with_undated_last() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/issues"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    "[{}, {}, {}]",
                    issue_json(
                        1,
                        "PROJ-1",
                        "9月1日期限の課題",
                        "未対応",
                        9,
                        Some("2024-09-01T00:00:00Z"),
                    ),
                    issue_json(2, "PROJ-2", "期限なしの課題", "処理中", 9, None),
                    issue_json(
                        3,
                        "PROJ-3",
                        "8月31日期限の課題",
                        "処理済み",
                        9,
                        Some("2024-08-31T00:00:00Z"),
                    ),
                ),
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.my_issues(&st, 1).await.unwrap();
        let keys: Vec<&str> = list.iter().map(|i| i.issue_key.as_str()).collect();
        // 期待順: 8/31(期限あり最早) → 9/1(期限あり) → 期限なし(末尾)
        assert_eq!(keys, vec!["PROJ-3", "PROJ-1", "PROJ-2"]);
        // フィールドの取り違え(summary/status.nameの入れ替わり)も検出できるよう
        // 個別に確認する。
        assert_eq!(list[0].summary, "8月31日期限の課題");
        assert_eq!(list[0].status_name, "処理済み");
        assert_eq!(list[1].summary, "9月1日期限の課題");
        assert_eq!(list[1].status_name, "未対応");
        assert_eq!(list[2].summary, "期限なしの課題");
        assert_eq!(list[2].status_name, "処理中");
    }

    /// 期限なしの課題どうしは、最終更新の新しい順に並ぶこと。
    ///
    /// 以前は `(None, None) => Equal` で API の返却順(実質は登録順)に
    /// 委ねていたため、期限なしグループの並びが無定義だった。
    #[tokio::test]
    async fn sorts_undated_issues_by_last_updated_desc() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/issues"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    "[{}, {}, {}]",
                    issue_json_with_times(
                        1,
                        "PROJ-1",
                        "更新が真ん中の課題",
                        "未対応",
                        9,
                        None,
                        "2024-01-01T00:00:00Z",
                        Some("2024-05-02T00:00:00Z"),
                    ),
                    issue_json_with_times(
                        2,
                        "PROJ-2",
                        "更新が最も古い課題",
                        "処理中",
                        9,
                        None,
                        "2024-01-01T00:00:00Z",
                        Some("2024-05-01T00:00:00Z"),
                    ),
                    issue_json_with_times(
                        3,
                        "PROJ-3",
                        "更新が最も新しい課題",
                        "処理済み",
                        9,
                        None,
                        "2024-01-01T00:00:00Z",
                        Some("2024-05-03T00:00:00Z"),
                    ),
                ),
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.my_issues(&st, 1).await.unwrap();
        let keys: Vec<&str> = list.iter().map(|i| i.issue_key.as_str()).collect();
        assert_eq!(keys, vec!["PROJ-3", "PROJ-1", "PROJ-2"]);
    }

    /// 最終更新が無い課題は、登録日時を最終更新とみなして並ぶこと。
    ///
    /// `updated` は API のモデルで `Option` なので、無い場合に `created` へ
    /// フォールバックしないと並びが崩れる。
    #[tokio::test]
    async fn falls_back_to_created_when_issue_has_never_been_updated() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/issues"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    "[{}, {}]",
                    // 更新済みだが、更新日時は下の課題の登録日時より古い。
                    issue_json_with_times(
                        1,
                        "PROJ-1",
                        "古く更新された課題",
                        "未対応",
                        9,
                        None,
                        "2024-01-01T00:00:00Z",
                        Some("2024-05-01T00:00:00Z"),
                    ),
                    // 一度も更新されていないが、登録が新しい。
                    issue_json_with_times(
                        2,
                        "PROJ-2",
                        "登録が新しく未更新の課題",
                        "処理中",
                        9,
                        None,
                        "2024-06-01T00:00:00Z",
                        None,
                    ),
                ),
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.my_issues(&st, 1).await.unwrap();
        let keys: Vec<&str> = list.iter().map(|i| i.issue_key.as_str()).collect();
        assert_eq!(keys, vec!["PROJ-2", "PROJ-1"]);
    }

    /// 期限が同じ日の課題どうしも、最終更新の新しい順に並ぶこと。
    #[tokio::test]
    async fn sorts_issues_with_the_same_due_date_by_last_updated_desc() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/issues"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    "[{}, {}]",
                    issue_json_with_times(
                        1,
                        "PROJ-1",
                        "同じ期限で更新が古い課題",
                        "未対応",
                        9,
                        Some("2024-08-31T00:00:00Z"),
                        "2024-01-01T00:00:00Z",
                        Some("2024-05-01T00:00:00Z"),
                    ),
                    issue_json_with_times(
                        2,
                        "PROJ-2",
                        "同じ期限で更新が新しい課題",
                        "処理中",
                        9,
                        Some("2024-08-31T00:00:00Z"),
                        "2024-01-01T00:00:00Z",
                        Some("2024-05-02T00:00:00Z"),
                    ),
                ),
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.my_issues(&st, 1).await.unwrap();
        let keys: Vec<&str> = list.iter().map(|i| i.issue_key.as_str()).collect();
        assert_eq!(keys, vec!["PROJ-2", "PROJ-1"]);
    }

    /// ウォッチが DTO に正しく変換され、`updated` が既読フラグの否定になること。
    #[tokio::test]
    async fn converts_watchings_and_inverts_the_read_flag() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/1/watchings"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    r#"[{{ "id": 2118167, "resourceAlreadyRead": false,
                           "lastContentUpdated": "2026-07-21T05:30:52Z",
                           "created": "2013-10-31T06:58:59Z",
                           "issue": {issue} }}]"#,
                    issue = issue_json(4531, "PROJ-17", "ウォッチ中の課題", "処理中", 9, None),
                ),
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let list = client.watchings(&st, 1).await.unwrap();
        let w = &list[0];
        assert_eq!(w.id, 2118167);
        // resourceAlreadyRead が false なので updated は true(否定であること)
        assert!(w.updated);
        assert_eq!(w.issue_key, "PROJ-17");
        assert_eq!(w.summary, "ウォッチ中の課題");
        assert_eq!(w.status_name, "処理中");
        assert_eq!(w.creator_id, Some(9));
        assert_eq!(w.creator_name, "作成者");
        assert_eq!(
            w.last_content_updated.as_deref(),
            Some("2026-07-21T05:30:52Z")
        );
        // `base_url` を両辺で使わない(finding 7 参照)。
        assert_eq!(w.url, format!("{}/view/PROJ-17", server.uri()));
    }

    /// 課題を持たないウォッチは一覧から落ちること(現状の filter_map と同じ)。
    #[tokio::test]
    async fn drops_watchings_without_an_issue() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/users/1/watchings"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[{ "id": 1, "resourceAlreadyRead": true, "issue": null,
                      "created": "2013-10-31T06:58:59Z" }]"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        assert!(client.watchings(&st, 1).await.unwrap().is_empty());
    }

    /// 通知が1件壊れていても、残りが DTO に変換されること。
    #[tokio::test]
    async fn notifications_keeps_the_good_items_and_counts_the_skipped() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[
                  { "id": 1, "alreadyRead": false, "reason": 2,
                    "resourceAlreadyRead": false, "created": "2026-07-30T07:55:07Z" },
                  { "id": 2, "alreadyRead": false },
                  { "id": 3, "alreadyRead": true, "reason": 4,
                    "resourceAlreadyRead": true, "created": "2026-07-29T07:55:07Z" }
                ]"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let page = client.notifications(&st, None).await.unwrap();
        assert_eq!(
            page.items.iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(page.skipped, 1);
    }

    /// 通知が2件壊れていても、両方まとめて数えられること。
    ///
    /// 既存の `notifications_keeps_the_good_items_and_counts_the_skipped` は
    /// 壊れた要素が1件だけの fixture だったため、`skipped` の集計が
    /// 「1件でも壊れていたら1」のような誤った実装でも通ってしまう
    /// (加算ではなく真偽値化する変異を検出できない)。2件壊す fixture で
    /// 固定する。
    #[tokio::test]
    async fn notifications_counts_two_malformed_items() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[
                  { "id": 1, "alreadyRead": false, "reason": 2,
                    "resourceAlreadyRead": false, "created": "2026-07-30T07:55:07Z" },
                  { "id": 2, "alreadyRead": false },
                  { "id": 3, "alreadyRead": true, "reason": 4,
                    "resourceAlreadyRead": true, "created": "2026-07-29T07:55:07Z" },
                  { "id": 4, "alreadyRead": false }
                ]"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let page = client.notifications(&st, None).await.unwrap();
        assert_eq!(
            page.items.iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(page.skipped, 2);
    }

    /// 正常時は skipped が 0。
    #[tokio::test]
    async fn notifications_reports_zero_skipped_for_a_clean_page() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[{ "id": 1, "alreadyRead": false, "reason": 2,
                      "resourceAlreadyRead": false, "created": "2026-07-30T07:55:07Z" }]"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let (client, st) = client_and_settings(&server, Lang::Ja);
        let page = client.notifications(&st, None).await.unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.skipped, 0);
    }
}
