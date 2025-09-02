use anyhow::{Context, Error, Result};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{Database, Iter};
use crate::api::issue::Issue;
use crate::outbound::issues::{
    IssueState, IssuesRepositoryIssuesNodes, IssuesRepositoryIssuesNodesAssignees,
    IssuesRepositoryIssuesNodesAuthor, IssuesRepositoryIssuesNodesAuthor::User as IssueAuthor,
    IssuesRepositoryIssuesNodesClosedByPullRequestsReferences,
    IssuesRepositoryIssuesNodesClosedByPullRequestsReferencesEdgesNode,
    IssuesRepositoryIssuesNodesClosedByPullRequestsReferencesEdgesNodeAuthor::User as PullRequestRefAuthor,
    IssuesRepositoryIssuesNodesComments, IssuesRepositoryIssuesNodesCommentsNodes,
    IssuesRepositoryIssuesNodesCommentsNodesAuthor::User as IssueCommentsAuthor,
    IssuesRepositoryIssuesNodesLabels, IssuesRepositoryIssuesNodesParent,
    IssuesRepositoryIssuesNodesProjectItems, IssuesRepositoryIssuesNodesProjectItemsNodes,
    IssuesRepositoryIssuesNodesProjectItemsNodesTodoInitiationOption as TodoInitOption,
    IssuesRepositoryIssuesNodesProjectItemsNodesTodoPendingDays as TodoPendingDays,
    IssuesRepositoryIssuesNodesProjectItemsNodesTodoPriority as TodoPriority,
    IssuesRepositoryIssuesNodesProjectItemsNodesTodoSize as TodoSize,
    IssuesRepositoryIssuesNodesProjectItemsNodesTodoStatus as TodoStatus,
    IssuesRepositoryIssuesNodesSubIssues, IssuesRepositoryIssuesNodesSubIssuesNodes,
    IssuesRepositoryIssuesNodesSubIssuesNodesAuthor::User as SubIssueAuthor, PullRequestState,
};

impl Database {
    pub(crate) fn insert_issues(
        &self,
        resp: Vec<GitHubIssue>,
        owner: &str,
        name: &str,
    ) -> Result<()> {
        for item in resp {
            let keystr: String = format!("{owner}/{name}#{}", item.number);
            Database::insert(&keystr, item, &self.issue_partition)?;
        }
        Ok(())
    }

    pub(crate) fn issues(&self, start: Option<&[u8]>, end: Option<&[u8]>) -> Iter<Issue> {
        let start = start.unwrap_or(b"\x00");
        if let Some(end) = end {
            Iter::new(self.issue_partition.range(start..end))
        } else {
            Iter::new(self.issue_partition.range(start..))
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubIssue {
    pub(crate) id: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) state: IssueState,
    pub(crate) assignees: Vec<String>,
    pub(crate) labels: Vec<String>,
    pub(crate) comments: GitHubIssueCommentConnection,
    pub(crate) project_items: GitHubProjectV2ItemConnection,
    pub(crate) sub_issues: GitHubSubIssueConnection,
    pub(crate) parent: Option<GitHubParentIssue>,
    pub(crate) url: String,
    pub(crate) closed_by_pull_requests: Vec<GitHubPullRequestRef>,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) closed_at: Option<Timestamp>,
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubIssueCommentConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<GitHubIssueComment>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubIssueComment {
    pub(crate) id: String,
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) repository_name: String,
    pub(crate) url: String,
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubProjectV2ItemConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<GitHubProjectV2Item>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubProjectV2Item {
    pub(crate) project_id: String,
    pub(crate) project_title: String,
    pub(crate) id: String,
    pub(crate) todo_status: Option<String>,
    pub(crate) todo_priority: Option<String>,
    pub(crate) todo_size: Option<String>,
    pub(crate) todo_initiation_option: Option<String>,
    pub(crate) todo_pending_days: Option<f64>,
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubSubIssueConnection {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<GitHubSubIssue>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubSubIssue {
    pub(crate) id: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) state: IssueState,
    pub(crate) author: String,
    pub(crate) assignees: Vec<String>,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) closed_at: Option<Timestamp>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubParentIssue {
    pub(crate) id: String,
    pub(crate) number: i32,
    pub(crate) title: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct GitHubPullRequestRef {
    pub(crate) number: i32,
    pub(crate) state: PullRequestState,
    pub(crate) author: String,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) closed_at: Option<Timestamp>,
    pub(crate) url: String,
}

/// Convert one single *Issue* of GitHub GraphQL API to our internal data structure (`GitHubIssue`)
impl TryFrom<IssuesRepositoryIssuesNodes> for GitHubIssue {
    type Error = Error;

    fn try_from(issue: IssuesRepositoryIssuesNodes) -> Result<Self> {
        let number: i32 = issue.number.try_into()?;
        let author = String::from(issue.author.context("Failed to fetch author of issue.")?);
        let comments = issue.comments.try_into()?;
        let project_items = issue.project_items.try_into()?;
        let sub_issues = issue.sub_issues.try_into()?;
        let parent = issue.parent.and_then(|node| node.try_into().ok());
        let closed_by_pull_requests = issue
            .closed_by_pull_requests_references
            .and_then(|pr| pr.try_into().ok())
            .unwrap_or_default();

        Ok(Self {
            id: issue.id,
            number,
            title: issue.title,
            author,
            body: issue.body,
            state: issue.state,
            assignees: issue.assignees.into(),
            labels: issue.labels.map(Vec::<String>::from).unwrap_or_default(),
            comments,
            project_items,
            sub_issues,
            parent,
            url: issue.url,
            closed_by_pull_requests,
            created_at: issue.created_at,
            updated_at: issue.updated_at,
            closed_at: issue.closed_at,
        })
    }
}

impl From<IssuesRepositoryIssuesNodesAuthor> for String {
    fn from(author: IssuesRepositoryIssuesNodesAuthor) -> Self {
        match author {
            IssueAuthor(user) => user.login,
            _ => String::new(),
        }
    }
}

impl From<IssuesRepositoryIssuesNodesAssignees> for Vec<String> {
    fn from(assignees: IssuesRepositoryIssuesNodesAssignees) -> Self {
        assignees
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|user| user.login)
            .collect()
    }
}

impl From<IssuesRepositoryIssuesNodesLabels> for Vec<String> {
    fn from(labels: IssuesRepositoryIssuesNodesLabels) -> Self {
        labels
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|label| label.name)
            .collect()
    }
}

impl TryFrom<IssuesRepositoryIssuesNodesComments> for GitHubIssueCommentConnection {
    type Error = Error;

    fn try_from(comments: IssuesRepositoryIssuesNodesComments) -> Result<Self> {
        let total_count = comments.total_count.try_into()?;

        Ok(Self {
            total_count,
            nodes: comments
                .nodes
                .unwrap_or_default()
                .into_iter()
                .flatten()
                .map(GitHubIssueComment::from)
                .collect(),
        })
    }
}

impl From<IssuesRepositoryIssuesNodesCommentsNodes> for GitHubIssueComment {
    fn from(comment: IssuesRepositoryIssuesNodesCommentsNodes) -> Self {
        Self {
            author: match comment.author {
                Some(IssueCommentsAuthor(u)) => u.login,
                _ => String::new(),
            },
            body: comment.body,
            created_at: comment.created_at,
            id: comment.id,
            repository_name: comment.repository.name,
            updated_at: comment.updated_at,
            url: comment.url,
        }
    }
}

impl TryFrom<IssuesRepositoryIssuesNodesProjectItems> for GitHubProjectV2ItemConnection {
    type Error = Error;

    fn try_from(project_items: IssuesRepositoryIssuesNodesProjectItems) -> Result<Self> {
        let total_count = project_items.total_count.try_into()?;

        Ok(Self {
            total_count,
            nodes: project_items
                .nodes
                .unwrap_or_default()
                .into_iter()
                .flatten()
                .map(GitHubProjectV2Item::from)
                .collect(),
        })
    }
}

impl From<IssuesRepositoryIssuesNodesProjectItemsNodes> for GitHubProjectV2Item {
    fn from(node: IssuesRepositoryIssuesNodesProjectItemsNodes) -> Self {
        Self {
            project_id: node.project.id,
            project_title: node.project.title,
            id: node.id,
            todo_status: node.todo_status.and_then(|status| match status {
                TodoStatus::ProjectV2ItemFieldSingleSelectValue(inner) => inner.name,
                _ => None,
            }),
            todo_priority: node.todo_priority.and_then(|priority| match priority {
                TodoPriority::ProjectV2ItemFieldSingleSelectValue(inner) => inner.name,
                _ => None,
            }),
            todo_size: node.todo_size.and_then(|size| match size {
                TodoSize::ProjectV2ItemFieldSingleSelectValue(inner) => inner.name,
                _ => None,
            }),
            todo_initiation_option: node.todo_initiation_option.and_then(|init| match init {
                TodoInitOption::ProjectV2ItemFieldSingleSelectValue(inner) => inner.name,
                _ => None,
            }),
            todo_pending_days: node.todo_pending_days.and_then(|days| match days {
                TodoPendingDays::ProjectV2ItemFieldNumberValue(inner) => inner.number,
                _ => None,
            }),
        }
    }
}

impl TryFrom<IssuesRepositoryIssuesNodesSubIssues> for GitHubSubIssueConnection {
    type Error = Error;

    fn try_from(sub_issues: IssuesRepositoryIssuesNodesSubIssues) -> Result<Self> {
        let total_count = sub_issues.total_count.try_into()?;
        let nodes = sub_issues
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(GitHubSubIssue::try_from)
            .collect::<Result<Vec<_>>>()?;

        Ok(Self { total_count, nodes })
    }
}

impl TryFrom<IssuesRepositoryIssuesNodesSubIssuesNodes> for GitHubSubIssue {
    type Error = Error;

    fn try_from(sub_issue: IssuesRepositoryIssuesNodesSubIssuesNodes) -> Result<Self> {
        let number = sub_issue.number.try_into()?;

        Ok(Self {
            id: sub_issue.id,
            number,
            title: sub_issue.title,
            state: sub_issue.state,
            created_at: sub_issue.created_at,
            updated_at: sub_issue.updated_at,
            closed_at: sub_issue.closed_at,
            author: match sub_issue.author {
                Some(SubIssueAuthor(u)) => u.login,
                _ => String::new(),
            },
            assignees: sub_issue
                .assignees
                .nodes
                .unwrap_or_default()
                .into_iter()
                .flatten()
                .map(|n| n.login)
                .collect(),
        })
    }
}

impl TryFrom<IssuesRepositoryIssuesNodesClosedByPullRequestsReferences>
    for Vec<GitHubPullRequestRef>
{
    type Error = Error;

    fn try_from(
        closing_prs: IssuesRepositoryIssuesNodesClosedByPullRequestsReferences,
    ) -> Result<Self> {
        closing_prs
            .edges
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter_map(|edge| edge.node.map(GitHubPullRequestRef::try_from))
            .collect::<Result<Vec<_>>>()
    }
}

impl TryFrom<IssuesRepositoryIssuesNodesClosedByPullRequestsReferencesEdgesNode>
    for GitHubPullRequestRef
{
    type Error = Error;

    fn try_from(
        node: IssuesRepositoryIssuesNodesClosedByPullRequestsReferencesEdgesNode,
    ) -> std::result::Result<Self, Self::Error> {
        let number = node.number.try_into()?;

        Ok(Self {
            number,
            state: node.state,
            created_at: node.created_at,
            updated_at: node.updated_at,
            closed_at: node.closed_at,
            author: match node.author {
                Some(PullRequestRefAuthor(u)) => u.login,
                _ => String::new(),
            },
            url: node.url,
        })
    }
}

impl TryFrom<IssuesRepositoryIssuesNodesParent> for GitHubParentIssue {
    type Error = Error;

    fn try_from(parent: IssuesRepositoryIssuesNodesParent) -> Result<Self> {
        let number = parent.number.try_into()?;

        Ok(Self {
            id: parent.id,
            number,
            title: parent.title,
        })
    }
}
