use std::time::Duration;

use archodin::graph::{JobOrigin, JobQueue, ManualClock, ScheduleError, Scheduler, Timetable};

const DAY: Duration = Duration::from_secs(24 * 3600);

#[test]
fn agora_jobs_start_before_issue_jobs_and_ties_keep_submission_order() {
    let queue = JobQueue::new();
    queue.submit(JobOrigin::Issue, "issue-1");
    queue.submit(JobOrigin::Agora, "agora-1");
    queue.submit(JobOrigin::Issue, "issue-2");
    queue.submit(JobOrigin::Agora, "agora-2");

    let order: Vec<_> = std::iter::from_fn(|| queue.next())
        .map(|job| job.state)
        .collect();
    assert_eq!(order, ["agora-1", "agora-2", "issue-1", "issue-2"]);
    assert!(queue.is_empty());
}

#[test]
fn a_late_agora_job_jumps_ahead_of_waiting_issue_jobs() {
    let queue = JobQueue::new();
    queue.submit(JobOrigin::Issue, "issue");
    assert_eq!(queue.len(), 1);
    queue.submit(JobOrigin::Agora, "agora");
    assert_eq!(queue.next().unwrap().state, "agora");
}

#[test]
fn a_scheduled_job_runs_at_its_times_and_each_run_is_its_own_job() {
    let clock = ManualClock::new();
    let queue = JobQueue::new();
    let scheduler = Scheduler::new(clock.clone(), queue.clone());
    let schedule = scheduler.schedule(Timetable::every(DAY), JobOrigin::Agora, "daily report");

    scheduler.tick();
    assert!(queue.is_empty(), "nothing is due before the first time");

    clock.advance(DAY);
    scheduler.tick();
    scheduler.tick();
    assert_eq!(queue.len(), 1, "one run per time, not per tick");

    clock.advance(DAY);
    scheduler.tick();
    let first = queue.next().unwrap();
    let second = queue.next().unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(first.state, "daily report");
    assert_eq!(first.schedule, Some(schedule));
    assert_eq!(second.schedule, Some(schedule));
}

#[test]
fn a_paused_scheduled_job_stops_and_resumes_without_replaying_missed_runs() {
    let clock = ManualClock::new();
    let queue = JobQueue::new();
    let scheduler = Scheduler::new(clock.clone(), queue.clone());
    let schedule = scheduler.schedule(Timetable::every(DAY), JobOrigin::Issue, "report");

    scheduler.pause(schedule).unwrap();
    clock.advance(DAY * 3);
    scheduler.tick();
    assert!(queue.is_empty());

    scheduler.resume(schedule).unwrap();
    scheduler.tick();
    assert!(queue.is_empty(), "missed runs are not replayed");

    clock.advance(DAY);
    scheduler.tick();
    assert_eq!(queue.len(), 1);
}

#[test]
fn a_tick_after_a_long_gap_queues_every_missed_time() {
    let clock = ManualClock::new();
    let queue = JobQueue::new();
    let scheduler = Scheduler::new(clock.clone(), queue.clone());
    scheduler.schedule(Timetable::every(DAY), JobOrigin::Issue, "report");
    clock.advance(DAY * 3);
    scheduler.tick();
    assert_eq!(queue.len(), 3);
}

#[test]
fn pausing_an_unknown_schedule_is_an_error() {
    let scheduler: Scheduler<&str> = Scheduler::new(ManualClock::new(), JobQueue::new());
    let other = Scheduler::new(ManualClock::new(), JobQueue::<&str>::new()).schedule(
        Timetable::every(DAY),
        JobOrigin::Issue,
        "x",
    );
    assert_eq!(scheduler.pause(other), Err(ScheduleError::Unknown(other)));
}
