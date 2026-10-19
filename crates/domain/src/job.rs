//! A job: one song being made (SPEC 5, jobs).

use serde::Serialize;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Waiting,
    Making,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    /// Waiting or making: still running, and still cancellable.
    pub fn is_open(self) -> bool {
        matches!(self, Self::Waiting | Self::Making)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Job {
    pub id: String,
    pub state: JobState,
    /// 0 to 1 while making.
    pub progress: f32,
    pub eta_s: Option<u32>,
    /// The person's prompt, else the mood: what "Being made" shows.
    pub label: String,
    pub song_id: Option<String>,
    pub error: Option<String>,
    /// Something that happened along the way, such as a lyrics fallback.
    pub note: Option<String>,
    /// A `limited` engine licence's terms (SPEC 5, licences).
    pub licence_note: Option<String>,
    pub created_at: String,
}

impl Job {
    pub fn new(id: String, label: String, eta_s: Option<u32>, created_at: String) -> Self {
        Self { id, state: JobState::Waiting, progress: 0.0, eta_s, label, song_id: None, error: None, note: None, licence_note: None, created_at }
    }

    pub fn cancel(&mut self) -> Result<(), Error> {
        if !self.state.is_open() {
            return Err(Error::invalid("job_finished", "the job has already finished"));
        }
        self.state = JobState::Cancelled;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_job_waits_and_only_open_jobs_can_be_cancelled() {
        let mut job = Job::new("j1".into(), "warm pads".into(), Some(120), "t".into());
        assert_eq!((job.state, job.progress, job.eta_s), (JobState::Waiting, 0.0, Some(120)));
        job.state = JobState::Making;
        job.cancel().unwrap();
        assert_eq!(job.state, JobState::Cancelled);
        assert_eq!(job.cancel().unwrap_err().code(), "job_finished");
        for state in [JobState::Done, JobState::Failed] {
            let mut finished = Job { state, ..Job::new("j".into(), "x".into(), None, "t".into()) };
            assert_eq!(finished.cancel().unwrap_err().code(), "job_finished");
            assert_eq!(finished.state, state);
        }
    }

    #[test]
    fn only_waiting_and_making_are_open() {
        assert!(JobState::Waiting.is_open() && JobState::Making.is_open());
        assert!(!JobState::Done.is_open() && !JobState::Failed.is_open() && !JobState::Cancelled.is_open());
    }
}
