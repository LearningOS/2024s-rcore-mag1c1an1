//! Process management syscalls
use core::mem::{size_of, transmute};

use crate::{
    config::MAX_SYSCALL_NUM,
    mm::translated_byte_buffer,
    task::{
        change_program_brk, current_user_token, exit_current_and_run_next, mmap, munmap,
        suspend_current_and_run_next, task_info, TaskStatus,
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");

    let buffers =
        translated_byte_buffer(current_user_token(), ts as *const u8, size_of::<TimeVal>());
    let us = get_time_us();
    let tmp = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    unsafe {
        let src: [u8; size_of::<TimeVal>()] = transmute(tmp);
        let mut idx = 0;
        for buffer in buffers {
            for b in buffer {
                *b = src[idx];
                idx += 1;
            }
        }
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info!");
    let (status, syscall_times, time) = task_info();
    let tmp = TaskInfo {
        status,
        syscall_times,
        time,
    };
    let buffers =
        translated_byte_buffer(current_user_token(), ti as *const u8, size_of::<TaskInfo>());
    unsafe {
        // copy to buffer
        //  may not contiuous memory
        let src: [u8; size_of::<TaskInfo>()] = transmute(tmp);
        let mut idx = 0;
        for buffer in buffers {
            for b in buffer {
                *b = src[idx];
                idx += 1;
            }
        }
    }
    0
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    mmap(start, len, port)
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    munmap(start, len)
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
