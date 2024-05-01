use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    let tid = current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid;
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        tid,
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.available_mutex[id] = Some(1);
        process_inner.mutex_list[id] = mutex;
        for tid in process_inner.tids() {
            *process_inner.allocation_mutex[tid]
                .as_mut()
                .unwrap()
                .get_mut(id)
                .unwrap() = Some(0);
            *process_inner.need_mutex[tid]
                .as_mut()
                .unwrap()
                .get_mut(id)
                .unwrap() = Some(0);
        }
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.available_mutex.push(Some(1));
        for tid in process_inner.tids() {
            process_inner.allocation_mutex[tid]
                .as_mut()
                .unwrap()
                .push(Some(0));
            process_inner.need_mutex[tid]
                .as_mut()
                .unwrap()
                .push(Some(0));
        }
        process_inner.mutex_list.len() as isize - 1
    }
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let tid = current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid;
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        tid,
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    *process_inner.need_mutex[tid].as_mut().unwrap()[mutex_id]
        .as_mut()
        .unwrap() += 1;
    if let Some(ans) = process_inner.mutex_deadlock_detect(mutex_id) {
        if ans {
            return -0xDEAD;
        }
    }
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);

    mutex.lock();

    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    *process_inner.available_mutex[mutex_id].as_mut().unwrap() -= 1;
    *process_inner.allocation_mutex[tid].as_mut().unwrap()[mutex_id]
        .as_mut()
        .unwrap() += 1;
    debug!("{:?}", process_inner.allocation_mutex[tid]);
    *process_inner.need_mutex[tid].as_mut().unwrap()[mutex_id]
        .as_mut()
        .unwrap() -= 1;
    0
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let tid = current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid;
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        tid,
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.unlock();

    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    *process_inner.available_mutex[mutex_id].as_mut().unwrap() += 1;
    *process_inner.allocation_mutex[tid].as_mut().unwrap()[mutex_id]
        .as_mut()
        .unwrap() -= 1;

    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    let tid = current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid;
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        tid,
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.available_sem[id] = Some(res_count);
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        for tid in process_inner.tids() {
            *process_inner.allocation_sem[tid]
                .as_mut()
                .unwrap()
                .get_mut(id)
                .unwrap() = Some(0);
            *process_inner.need_sem[tid]
                .as_mut()
                .unwrap()
                .get_mut(id)
                .unwrap() = Some(0);
        }
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.available_sem.push(Some(res_count));
        for tid in process_inner.tids() {
            process_inner.allocation_sem[tid]
                .as_mut()
                .unwrap()
                .push(Some(0));
            process_inner.need_sem[tid].as_mut().unwrap().push(Some(0));
        }
        debug!("{:?}", process_inner.need_sem);
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let tid = current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid;
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        tid,
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    sem.up();

    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    *process_inner.available_sem[sem_id].as_mut().unwrap() += 1;
    *process_inner.allocation_sem[tid].as_mut().unwrap()[sem_id]
        .as_mut()
        .unwrap() -= 1;

    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let tid = current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid;
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        tid,
    );
    debug!("sem_id: {sem_id}");
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    debug!("need_sem: {:?}", process_inner.need_sem);
    *process_inner.need_sem[tid].as_mut().unwrap()[sem_id]
        .as_mut()
        .unwrap() += 1;

    if let Some(ans) = process_inner.sem_deadlock_detect(sem_id) {
        if ans {
            return -0xDEAD;
        }
    }
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    sem.down();

    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    *process_inner.available_sem[sem_id].as_mut().unwrap() -= 1;
    *process_inner.allocation_sem[tid].as_mut().unwrap()[sem_id]
        .as_mut()
        .unwrap() += 1;
    *process_inner.need_sem[tid].as_mut().unwrap()[sem_id]
        .as_mut()
        .unwrap() -= 1;
    0
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}
/// enable deadlock detection syscall
///
/// YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(enabled: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_enable_deadlock_detect",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    match enabled {
        0 => {
            // disable
            assert!(process_inner.deadlock_detect);
            process_inner.deadlock_detect = false;
            0
        }
        1 => {
            assert!(!process_inner.deadlock_detect);
            process_inner.deadlock_detect = true;
            0
        }
        _ => -1,
    }
}
