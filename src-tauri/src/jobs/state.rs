use crate::contracts::{JobSnapshot, JobState};
pub fn is_terminal(state: JobState) -> bool {
    matches!(
        state,
        JobState::Succeeded | JobState::Failed | JobState::Canceled
    )
}
pub fn transition(snapshot: &mut JobSnapshot, next: JobState) -> bool {
    if is_terminal(snapshot.state) || snapshot.state == next {
        return false;
    }
    let valid = match (snapshot.state, next) {
        (JobState::Probing, JobState::Preparing)
        | (JobState::Preparing, JobState::Running)
        | (JobState::Running, JobState::Validating)
        | (JobState::Validating, JobState::Committing)
        | (JobState::Committing, JobState::Succeeded)
        | (JobState::Canceling, JobState::Canceled) => true,
        (s, JobState::Canceling) => !matches!(s, JobState::Committing | JobState::Canceling),
        (_, JobState::Failed) => true,
        _ => false,
    };
    if valid {
        snapshot.state = next;
        snapshot.version += 1;
    }
    valid
}
