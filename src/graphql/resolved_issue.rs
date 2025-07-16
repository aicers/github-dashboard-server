use std::fmt;

use anyhow::anyhow;
use async_graphql::{
    connection::{Connection, Edge},
    Context, Object, SimpleObject,
};
use base64::{engine::general_purpose, Engine as _};

use crate::{
    database::{Database, TryFromKeyValue},
    github::issues::{IssueState, PullRequestState},
    graphql::issue::Issue,
    graphql::issue_stat::IssueStatFilter,
    graphql::total_count_field::TotalCountField,
};

#[derive(SimpleObject)]
pub(crate) struct ResolvedIssue {
    #[graphql(flatten)]
    issue: Issue,
}

impl Issue {
    pub(super) fn is_resolved(&self) -> bool {
        if self.state == IssueState::OPEN {
            // If an issue is open, we can conclude that the issue is not resolved now
            return false;
        }

        if (!self.closed_by_pull_requests.is_empty())
            && self
                .closed_by_pull_requests
                .iter()
                .all(|closing_pr| closing_pr.state == PullRequestState::MERGED)
        {
            // If the issue has closing PR, we can conclude that the issue is resolved
            return true;
        }

        false
    }
}

impl ResolvedIssue {
    pub(super) fn load(ctx: &Context<'_>, filter: &IssueStatFilter) -> Vec<ResolvedIssue> {
        // Load all issues which meet filter condition
        if let Ok(db) = ctx.data::<Database>() {
            let issues = db.issues(None, None);
            let filtered = filter.filter_issues(issues);

            // Select resolved issues among issues
            filtered
                .iter()
                .filter_map(|issue| ResolvedIssue::try_from(issue).ok())
                .collect()
        } else {
            vec![]
        }
    }
}

impl TryFrom<Issue> for ResolvedIssue {
    type Error = anyhow::Error;

    fn try_from(issue: Issue) -> anyhow::Result<Self> {
        if issue.is_resolved() {
            Ok(Self { issue })
        } else {
            Err(anyhow!(
                "Error converting issue to resolved issue. The issue is not resolved"
            ))
        }
    }
}

impl TryFrom<&Issue> for ResolvedIssue {
    type Error = anyhow::Error;

    fn try_from(issue_ref: &Issue) -> anyhow::Result<Self> {
        Self::try_from(issue_ref.to_owned())
    }
}

impl TryFromKeyValue for ResolvedIssue {
    fn try_from_key_value(key: &[u8], value: &[u8]) -> anyhow::Result<Self> {
        let issue = Issue::try_from_key_value(key, value)?;
        Ok(Self { issue })
    }
}

impl fmt::Display for ResolvedIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.issue.fmt(f)
    }
}

#[derive(Default)]
pub(super) struct ResolvedIssueQuery;

#[Object]
impl ResolvedIssueQuery {
    #[allow(clippy::unused_async)]
    async fn resolved_issues(
        &self,
        ctx: &Context<'_>,
        filter: IssueStatFilter,
    ) -> async_graphql::Result<Connection<String, ResolvedIssue, TotalCountField>> {
        let resolved_issues = ResolvedIssue::load(ctx, &filter);
        let total_count = resolved_issues.len();
        let mut connection =
            Connection::with_additional_fields(false, false, TotalCountField { total_count });

        for node in resolved_issues {
            connection.edges.push(Edge::new(
                general_purpose::STANDARD.encode(format!("{node}")),
                node,
            ));
        }

        Ok(connection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::issues::{IssueState, PullRequestState};
    use crate::graphql::issue::PullRequestRef;
    use crate::graphql::{
        issue::{Comment, CommentConnection, ProjectV2ItemConnection, SubIssueConnection},
        DateTimeUtc,
    };

    fn issue1() -> Issue {
        Issue {
            id: "I_kwDOHpM3FM5Nko9l".to_string(),
            owner: "aicers".to_string(),
            repo: "github-dashboard-server".to_string(),
            number: 1,
            title: "실행 파일 빌드 가능한 Cargo.toml 및 소스 파일 추가".to_string(),
            body: r#"프로젝트 디렉토리에서 `cargo run`을 실행하면 프로젝트 이름(\"AICE GitHub Dashboard Server\")를
            출력하고 종료하도록 Cargo.toml과 main.rs를 추가합니다. 코드는 `cargo clippy -- -D warnings -W
            clippy::pedantic`을 문제없이 통과할 수 있어야합니다.\r\n\r\n변경한 코드는 새로운 브랜치에 커밋하고 pull
            request로 올려주세요."#.to_string(),
            state: IssueState::CLOSED,
            author: "msk".to_string(),
            assignees: vec!["MontyCoder0701".to_string()],
            labels: vec![],
            comments: CommentConnection {
                total_count: 1,
                nodes: vec![
                    Comment {
                        id: "IC_kwDOHpM3FM5GaoOk".to_string(),
                        author: "msk".to_string(),
                        body: "Resolved in #3.".to_string(),
                        created_at: DateTimeUtc("2022-07-12T06:52:51Z".parse().unwrap()),
                        updated_at: DateTimeUtc("2022-07-12T06:52:51Z".parse().unwrap()),
                        repository_name: "github-dashboard-server".to_string(),
                        url: "https://github.com/aicers/github-dashboard-server/issues/1#issuecomment-1181385636".to_string()
                  }
                ]
            },
            project_items: ProjectV2ItemConnection {
                total_count: 0,
                nodes: vec![]
            },
            sub_issues: SubIssueConnection {
                total_count: 0,
                nodes: vec![]
            },
            parent: None,
            url: "https://github.com/aicers/github-dashboard-server/issues/1".to_string(),
            closed_by_pull_requests: vec![],
            created_at: DateTimeUtc("2022-07-12T02:26:17Z".parse().unwrap()),
            updated_at: DateTimeUtc("2022-07-12T06:52:51Z".parse().unwrap()),
            closed_at: Some(DateTimeUtc("2022-07-12T06:52:51Z".parse().unwrap())),
        }
    }

    fn issue4() -> Issue {
        Issue {
            id: "I_kwDOHpM3FM5NlYA_".to_string(),
            owner: "aicers".to_string(),
            repo: "github-dashboard-server".to_string(),
            number: 4,
            title: "웹서버 실행".to_string(),
            body: r"[warp](https://docs.rs/warp/latest/warp/)를 사용하여 현재 디렉터리의 파일 내용을 보여주는
            웹서버를 실행합니다.\r\n\r\nsrc 디렉터리에 `web` 모듈(web.rs)을 추가하고, `web::serve`라는 함수를 만들어
            웹서버를 시작합니다. 웹서버 시작은 [`warp::fs::dir`](https://docs.rs/warp/latest/warp/filters/fs/fn.dir.html)과
            [`warp::serve`](https://docs.rs/warp/latest/warp/fn.serve.html)를 쓰면 되는데, 주의할 점이 있습니다.
            `warp::serve`는 `async` 함수이므로 `web::serve`도 `async` 함수여야 합니다. 마찬가지로 `web::serve`를
            호출하는 `main`도 `async`여야 하는데, 이건 [`tokio::main`](https://docs.rs/tokio/latest/tokio/attr.main.html)을
            쓰면  됩니다.\r\n\r\n`cargo run` 실행 후 웹브라우저에서 `http://localhost:8000/README.md`로 접속하여
            이 프로젝트의 `README.md`의 내용이 나오면 성공입니다.".to_string(),
            state: IssueState::CLOSED,
            author: "msk".to_string(),
            assignees: vec!["BLYKIM".to_string()],
            labels: vec![],
            comments: CommentConnection {
                total_count: 0,
                nodes: vec![]
            },
            project_items: ProjectV2ItemConnection {
                total_count: 0,
                nodes: vec![]
            },
            sub_issues: SubIssueConnection {
                total_count: 0,
                nodes: vec![]
            },
            parent: None,
            url: "https://github.com/aicers/github-dashboard-server/issues/4".to_string(),
            closed_by_pull_requests: vec![
                PullRequestRef {
                    number: 5,
                    state: PullRequestState::MERGED,
                    author: "BLYKIM".to_string(),
                    created_at: DateTimeUtc("2022-07-12T08:27:43Z".parse().unwrap()),
                    closed_at: Some(DateTimeUtc("2022-07-12T09:09:01Z".parse().unwrap())),
                    updated_at: DateTimeUtc("2022-07-12T09:09:01Z".parse().unwrap()),
                    url:"https://github.com/aicers/github-dashboard-server/pull/5".to_string()
                }
            ],
            created_at: DateTimeUtc("2022-07-12T07:16:32Z".parse().unwrap()),
            updated_at: DateTimeUtc("2022-07-12T09:09:01Z".parse().unwrap()),
            closed_at: Some(DateTimeUtc("2022-07-12T09:09:01Z".parse().unwrap())),
        }
    }

    #[test]
    fn determine_whether_issue_is_resolved() {
        let issue1 = issue1();
        let issue4 = issue4();

        assert!(!issue1.is_resolved());
        assert!(issue4.is_resolved());
    }

    #[test]
    fn convert_issue_to_resolved_issue() {
        let issue1 = issue1();
        let issue4 = issue4();

        assert!(<ResolvedIssue>::try_from(issue1).is_err());
        assert!(<ResolvedIssue>::try_from(issue4).is_ok());
    }

    #[test]
    fn convert_issue_to_resolved_issue_ref() {
        let issue1_ref = &issue1();
        let issue4_ref = &issue4();

        assert!(<ResolvedIssue>::try_from(issue1_ref).is_err());
        assert!(<ResolvedIssue>::try_from(issue4_ref).is_ok());
    }
}
