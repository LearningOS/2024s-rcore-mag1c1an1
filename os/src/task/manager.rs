//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        debug!("[");
        for t in self.ready_queue.iter() {
            debug!("   {}", t.stride());
        }
        debug!("]");

        if self.ready_queue.is_empty() {
            debug!("None");
            None
        } else {
            let mut idx = 0usize;
            for i in 1..self.ready_queue.len() {
                match self.ready_queue[idx]
                    .stride()
                    .partial_cmp(&self.ready_queue[i].stride())
                {
                    Some(ord) => match ord {
                        core::cmp::Ordering::Less => {}
                        core::cmp::Ordering::Equal => todo!(),
                        core::cmp::Ordering::Greater => idx = i,
                    },
                    None => todo!(),
                }
            }
            debug!("idx: {idx}");
            self.ready_queue.remove(idx)
        }
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    //trace!("kernel: TaskManager::add_task");
    TASK_MANAGER.exclusive_access().add(task);
}

/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}
