use std::collections::HashMap;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

#[derive(Debug, Clone, PartialEq)]
pub enum JobStatus {
    Running,
    Stopped,
    Done,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id:      usize,
    pub pid:     u32,
    pub command: String,
    pub status:  JobStatus,
}

pub struct JobTable {
    jobs:    HashMap<usize, Job>,
    next_id: usize,
}

impl JobTable {
    pub fn new() -> Self {
        JobTable {
            jobs:    HashMap::new(),
            next_id: 1,
        }
    }

    pub fn add(&mut self, pid: u32, command: &str) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.jobs.insert(
            id,
            Job {
                id,
                pid,
                command: command.to_string(),
                         status: JobStatus::Running,
            },
        );
        println!("[{}] {}", id, pid);
        id
    }

    pub fn list(&self) {
        let mut ids: Vec<usize> = self.jobs.keys().cloned().collect();
        ids.sort();
        for id in ids {
            if let Some(job) = self.jobs.get(&id) {
                let status = match job.status {
                    JobStatus::Running => "Running",
                    JobStatus::Stopped => "Stopped",
                    JobStatus::Done    => "Done",
                };
                println!("[{}] {:8}  {}  {}", job.id, status, job.pid, job.command);
            }
        }
    }

    pub fn fg(&mut self, id: usize) -> Option<u32> {
        self.jobs.get(&id).map(|j| j.pid)
    }

    pub fn remove(&mut self, id: usize) {
        self.jobs.remove(&id);
    }

    pub fn mark_done(&mut self, id: usize) {
        if let Some(j) = self.jobs.get_mut(&id) {
            j.status = JobStatus::Done;
        }
    }

    pub fn send_signal(&self, id: usize, sig: Signal) -> bool {
        if let Some(job) = self.jobs.get(&id) {
            kill(Pid::from_raw(job.pid as i32), sig).is_ok()
        } else {
            false
        }
    }

    /// Poll all background jobs non-blockingly; print notification for finished ones.
    pub fn check_finished(&mut self) {
        use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};

        let ids: Vec<usize> = self.jobs.keys().cloned().collect();
        for id in ids {
            let pid = match self.jobs.get(&id) {
                Some(j) => Pid::from_raw(j.pid as i32),
                None    => continue,
            };

            match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(_, code)) => {
                    if let Some(job) = self.jobs.remove(&id) {
                        println!(
                            "\n\x1b[1;32m[{}] Done ({})\x1b[0m  {}",
                                 job.id, code, job.command
                        );
                    }
                }
                Ok(WaitStatus::Signaled(_, sig, _)) => {
                    if let Some(job) = self.jobs.remove(&id) {
                        println!(
                            "\n\x1b[1;31m[{}] Killed ({})\x1b[0m  {}",
                                 job.id, sig, job.command
                        );
                    }
                }
                _ => {} // Still running, or WNOHANG returned "not yet"
            }
        }
    }
}
