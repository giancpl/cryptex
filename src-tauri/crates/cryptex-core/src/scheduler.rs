use crate::{
    api::{BuildReason, OperationId},
    process::CancellationToken,
};
use std::collections::HashMap;
use thiserror::Error;

pub struct ScheduledBuild<T> {
    pub operation_id: OperationId,
    pub project_id: String,
    pub reason: BuildReason,
    pub request: T,
    pub cancellation: CancellationToken,
}

pub enum ScheduleDisposition<T> {
    Started(ScheduledBuild<T>),
    Queued {
        operation_id: OperationId,
        superseded: Option<OperationId>,
        cancelled_active: Option<OperationId>,
    },
    Coalesced {
        operation_id: OperationId,
        into: OperationId,
    },
}

pub enum CompletionDisposition<T> {
    Accepted {
        publish_result: bool,
        next: Option<ScheduledBuild<T>>,
    },
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelDisposition {
    Active,
    Queued,
    NotFound,
}

struct ActiveBuild {
    operation_id: OperationId,
    reason: BuildReason,
    cancellation: CancellationToken,
    suppress_result: bool,
}

struct QueuedBuild<T> {
    operation_id: OperationId,
    reason: BuildReason,
    request: T,
}

struct ProjectQueue<T> {
    active: Option<ActiveBuild>,
    queued: Option<QueuedBuild<T>>,
}

impl<T> Default for ProjectQueue<T> {
    fn default() -> Self {
        Self {
            active: None,
            queued: None,
        }
    }
}

pub struct BuildScheduler<T> {
    next_operation: u64,
    projects: HashMap<String, ProjectQueue<T>>,
}

impl<T> Default for BuildScheduler<T> {
    fn default() -> Self {
        Self {
            next_operation: 1,
            projects: HashMap::new(),
        }
    }
}

impl<T> BuildScheduler<T> {
    pub fn enqueue(
        &mut self,
        project_id: String,
        reason: BuildReason,
        request: T,
    ) -> Result<ScheduleDisposition<T>, SchedulerError> {
        let operation_id = self.operation_id()?;
        let queue = self.projects.entry(project_id.clone()).or_default();
        if queue.active.is_none() {
            let scheduled = activate(queue, project_id, operation_id, reason, request);
            return Ok(ScheduleDisposition::Started(scheduled));
        }

        if reason == BuildReason::Save
            && let Some(queued) = &queue.queued
            && queued.reason == BuildReason::Explicit
        {
            return Ok(ScheduleDisposition::Coalesced {
                operation_id,
                into: queued.operation_id.clone(),
            });
        }

        let cancelled_active = queue.active.as_mut().and_then(|active| {
            if reason == BuildReason::Explicit && active.reason == BuildReason::Save {
                active.suppress_result = true;
                active.cancellation.cancel();
                Some(active.operation_id.clone())
            } else {
                None
            }
        });
        let superseded = queue
            .queued
            .replace(QueuedBuild {
                operation_id: operation_id.clone(),
                reason,
                request,
            })
            .map(|queued| queued.operation_id);
        Ok(ScheduleDisposition::Queued {
            operation_id,
            superseded,
            cancelled_active,
        })
    }

    pub fn complete(
        &mut self,
        project_id: &str,
        operation_id: &OperationId,
    ) -> CompletionDisposition<T> {
        let Some(queue) = self.projects.get_mut(project_id) else {
            return CompletionDisposition::Stale;
        };
        let Some(active) = &queue.active else {
            return CompletionDisposition::Stale;
        };
        if active.operation_id != *operation_id {
            return CompletionDisposition::Stale;
        }
        let publish_result = !active.suppress_result;
        queue.active = None;
        let next = queue.queued.take().map(|queued| {
            activate(
                queue,
                project_id.to_owned(),
                queued.operation_id,
                queued.reason,
                queued.request,
            )
        });
        if queue.active.is_none() && queue.queued.is_none() {
            self.projects.remove(project_id);
        }
        CompletionDisposition::Accepted {
            publish_result,
            next,
        }
    }

    pub fn cancel(&mut self, project_id: &str, operation_id: &OperationId) -> CancelDisposition {
        let Some(queue) = self.projects.get_mut(project_id) else {
            return CancelDisposition::NotFound;
        };
        if let Some(active) = &queue.active
            && active.operation_id == *operation_id
        {
            active.cancellation.cancel();
            return CancelDisposition::Active;
        }
        if queue
            .queued
            .as_ref()
            .is_some_and(|queued| queued.operation_id == *operation_id)
        {
            queue.queued = None;
            return CancelDisposition::Queued;
        }
        CancelDisposition::NotFound
    }

    pub fn active_operation(&self, project_id: &str) -> Option<&OperationId> {
        self.projects
            .get(project_id)
            .and_then(|queue| queue.active.as_ref())
            .map(|active| &active.operation_id)
    }

    fn operation_id(&mut self) -> Result<OperationId, SchedulerError> {
        let value = self.next_operation;
        self.next_operation = self
            .next_operation
            .checked_add(1)
            .ok_or(SchedulerError::OperationIdExhausted)?;
        Ok(OperationId(format!("build-{value:020}")))
    }
}

fn activate<T>(
    queue: &mut ProjectQueue<T>,
    project_id: String,
    operation_id: OperationId,
    reason: BuildReason,
    request: T,
) -> ScheduledBuild<T> {
    let cancellation = CancellationToken::default();
    queue.active = Some(ActiveBuild {
        operation_id: operation_id.clone(),
        reason,
        cancellation: cancellation.clone(),
        suppress_result: false,
    });
    ScheduledBuild {
        operation_id,
        project_id,
        reason,
        request,
        cancellation,
    }
}

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("build operation identifier space is exhausted")]
    OperationIdExhausted,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started<T>(disposition: ScheduleDisposition<T>) -> ScheduledBuild<T> {
        match disposition {
            ScheduleDisposition::Started(build) => build,
            _ => panic!("expected started build"),
        }
    }

    #[test]
    fn save_storm_keeps_one_active_and_only_the_latest_queued_request() {
        let mut scheduler = BuildScheduler::default();
        let first = started(
            scheduler
                .enqueue("project".to_owned(), BuildReason::Save, "first")
                .unwrap(),
        );
        let second_id = match scheduler
            .enqueue("project".to_owned(), BuildReason::Save, "second")
            .unwrap()
        {
            ScheduleDisposition::Queued { operation_id, .. } => operation_id,
            _ => panic!("expected queued build"),
        };
        match scheduler
            .enqueue("project".to_owned(), BuildReason::Save, "latest")
            .unwrap()
        {
            ScheduleDisposition::Queued {
                superseded: Some(superseded),
                ..
            } => assert_eq!(superseded, second_id),
            _ => panic!("expected replacement"),
        }
        match scheduler.complete("project", &first.operation_id) {
            CompletionDisposition::Accepted {
                publish_result: true,
                next: Some(next),
            } => assert_eq!(next.request, "latest"),
            _ => panic!("expected latest build to start"),
        }
    }

    #[test]
    fn explicit_build_preempts_active_save_and_suppresses_its_stale_result() {
        let mut scheduler = BuildScheduler::default();
        let save = started(
            scheduler
                .enqueue("project".to_owned(), BuildReason::Save, "save")
                .unwrap(),
        );
        match scheduler
            .enqueue("project".to_owned(), BuildReason::Explicit, "explicit")
            .unwrap()
        {
            ScheduleDisposition::Queued {
                cancelled_active: Some(cancelled),
                ..
            } => assert_eq!(cancelled, save.operation_id),
            _ => panic!("expected explicit build to preempt save"),
        }
        assert!(save.cancellation.is_cancelled());
        match scheduler.complete("project", &save.operation_id) {
            CompletionDisposition::Accepted {
                publish_result: false,
                next: Some(next),
            } => {
                assert_eq!(next.reason, BuildReason::Explicit);
                assert_eq!(next.request, "explicit");
            }
            _ => panic!("expected suppressed save and explicit successor"),
        }
    }

    #[test]
    fn saves_coalesce_into_a_queued_explicit_build_without_replacing_it() {
        let mut scheduler = BuildScheduler::default();
        let active = started(
            scheduler
                .enqueue("project".to_owned(), BuildReason::Explicit, "active")
                .unwrap(),
        );
        let explicit_id = match scheduler
            .enqueue("project".to_owned(), BuildReason::Explicit, "next explicit")
            .unwrap()
        {
            ScheduleDisposition::Queued { operation_id, .. } => operation_id,
            _ => panic!("expected queued explicit build"),
        };
        match scheduler
            .enqueue("project".to_owned(), BuildReason::Save, "save")
            .unwrap()
        {
            ScheduleDisposition::Coalesced { into, .. } => assert_eq!(into, explicit_id),
            _ => panic!("expected save to coalesce"),
        }
        match scheduler.complete("project", &active.operation_id) {
            CompletionDisposition::Accepted {
                next: Some(next), ..
            } => assert_eq!(next.request, "next explicit"),
            _ => panic!("expected explicit successor"),
        }
    }

    #[test]
    fn cancellation_and_stale_completion_are_scoped_by_operation() {
        let mut scheduler = BuildScheduler::default();
        let active = started(
            scheduler
                .enqueue("a".to_owned(), BuildReason::Explicit, "active")
                .unwrap(),
        );
        let queued_id = match scheduler
            .enqueue("a".to_owned(), BuildReason::Save, "queued")
            .unwrap()
        {
            ScheduleDisposition::Queued { operation_id, .. } => operation_id,
            _ => panic!("expected queued build"),
        };
        assert_eq!(scheduler.cancel("a", &queued_id), CancelDisposition::Queued);
        assert_eq!(
            scheduler.cancel("a", &active.operation_id),
            CancelDisposition::Active
        );
        assert!(active.cancellation.is_cancelled());
        assert!(matches!(
            scheduler.complete("a", &OperationId("build-stale".to_owned())),
            CompletionDisposition::Stale
        ));
        assert_eq!(scheduler.active_operation("a"), Some(&active.operation_id));
    }

    #[test]
    fn different_projects_can_run_independently_and_ids_are_monotonic() {
        let mut scheduler = BuildScheduler::default();
        let first = started(
            scheduler
                .enqueue("a".to_owned(), BuildReason::Save, 1)
                .unwrap(),
        );
        let second = started(
            scheduler
                .enqueue("b".to_owned(), BuildReason::Save, 2)
                .unwrap(),
        );
        assert!(first.operation_id.0 < second.operation_id.0);
        assert_eq!(scheduler.active_operation("a"), Some(&first.operation_id));
        assert_eq!(scheduler.active_operation("b"), Some(&second.operation_id));
    }
}
