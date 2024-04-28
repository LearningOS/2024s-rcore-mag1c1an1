//! Types related to task management

use crate::config::MAX_SYSCALL_NUM;

use super::TaskContext;
use super::{kstack_alloc, pid_alloc, KernelStack, PidHandle};
use crate::config::TRAP_CONTEXT_BASE;
use crate::mm::{
    MapPermission, MemorySet, PhysPageNum, VPNRange, VirtAddr, VirtPageNum, KERNEL_SPACE,
};
use crate::sync::UPSafeCell;
use crate::trap::{trap_handler, TrapContext};
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::cell::RefMut;
use core::cmp::Ordering;
use core::fmt::Display;

/// Task control block structure
///
/// Directly save the contents that will not change during running
pub struct TaskControlBlock {
    // Immutable
    /// Process identifier
    pub pid: PidHandle,

    /// Kernel stack corresponding to PID
    pub kernel_stack: KernelStack,

    /// Mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

impl TaskControlBlock {
    /// Get the mutable reference of the inner TCB
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }
    /// Get the address of app's page table
    pub fn get_user_token(&self) -> usize {
        let inner = self.inner_exclusive_access();
        inner.memory_set.token()
    }
    /// dummy
    pub fn task_info(&self) -> (TaskStatus, [u32; MAX_SYSCALL_NUM], usize) {
        let inner = self.inner_exclusive_access();
        (inner.task_status, inner.syscall_times, inner.start_time)
    }

    /// dummy
    pub fn mmap(&self, start: usize, len: usize, port: usize) -> isize {
        let mut inner = self.inner.exclusive_access();
        let ms = &mut inner.memory_set;

        let sva = VirtAddr(start);
        let eva = VirtAddr(start + len);
        if !sva.aligned() {
            error!("va is not aligned");
            return -1;
        }

        if port & !0x7 != 0 {
            error!("port error");
            return -1;
        }

        if port & 0x7 == 0 {
            error!("meaningless mmmp");
            return -1;
        }

        let svpn = VirtPageNum::from(sva);
        let evpn = eva.ceil();
        let vpn_range = VPNRange::new(svpn, evpn);
        for vpn in vpn_range {
            if let Some(pte) = ms.translate(vpn) {
                if pte.is_valid() {
                    error!("vpn has been used");
                    return -1;
                }
            }
        }

        let perm = MapPermission::from_bits((port as u8) << 1).unwrap() | MapPermission::U;
        ms.insert_framed_area(VirtAddr(start), VirtAddr(start + len), perm);
        0
    }

    /// dummy
    pub fn munmap(&self, start: usize, len: usize) -> isize {
        let mut inner = self.inner.exclusive_access();
        let ms = &mut inner.memory_set;

        let sva = VirtAddr(start);
        let eva = VirtAddr(start + len);

        if !sva.aligned() {
            error!("{}:{} start is not aligned", file!(), line!());
            return -1;
        }

        let svpn = VirtPageNum::from(sva);
        let evpn = eva.ceil();
        let vpn_range = VPNRange::new(svpn, evpn);
        for vpn in vpn_range {
            if let Some(pte) = ms.translate(vpn) {
                if !pte.is_valid() {
                    error!("{}:{} pte is not valid", file!(), line!());
                    return -1;
                }
            }
        }
        ms.munmap(vpn_range);
        0
    }

    /// dummy
    pub fn update_syscall_times(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        inner.syscall_times[syscall_id] += 1;
    }

    /// dummy
    pub fn add_pass(&self) {
        let mut inner = self.inner.exclusive_access();
        inner.stride.0 = inner.stride.0.wrapping_add(BIG_STRIDE / inner.priority)
    }

    /// dummy
    pub fn stride(&self) -> Stride {
        let inner = self.inner.exclusive_access();
        inner.stride
    }

    /// dummy
    pub fn set_priority(&self, priority: u64) {
        let mut inner = self.inner.exclusive_access();
        inner.priority = priority
    }
}

pub struct TaskControlBlockInner {
    /// The physical page number of the frame where the trap context is placed
    pub trap_cx_ppn: PhysPageNum,

    /// Application data can only appear in areas
    /// where the application address space is lower than base_size
    pub base_size: usize,

    /// Save task context
    pub task_cx: TaskContext,

    /// Maintain the execution status of the current process
    pub task_status: TaskStatus,

    /// Application address space
    pub memory_set: MemorySet,

    /// Parent process of the current process.
    /// Weak will not affect the reference count of the parent
    pub parent: Option<Weak<TaskControlBlock>>,

    /// A vector containing TCBs of all child processes of the current process
    pub children: Vec<Arc<TaskControlBlock>>,

    /// It is set when active exit or execution error occurs
    pub exit_code: i32,

    /// Heap bottom
    pub heap_bottom: usize,

    /// Program break
    pub program_brk: usize,

    ///  program start time (ticks)
    pub start_time: usize,

    /// record sycall times
    pub syscall_times: [u32; MAX_SYSCALL_NUM],

    /// record
    pub stride: Stride,

    /// process priority
    pub priority: u64,
}

impl TaskControlBlockInner {
    /// get the trap context
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }
    /// get the user token
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }
    pub fn is_zombie(&self) -> bool {
        self.get_status() == TaskStatus::Zombie
    }
}

impl TaskControlBlock {
    /// Create a new process
    ///
    /// At present, it is only used for the creation of initproc
    pub fn new(elf_data: &[u8]) -> Self {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        // push a task context which goes to trap_return to the top of kernel stack
        let task_control_block = Self {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    heap_bottom: user_sp,
                    program_brk: user_sp,
                    start_time: 0,
                    syscall_times: [0; MAX_SYSCALL_NUM],
                    stride: Stride(0),
                    priority: 16,
                })
            },
        };
        // prepare TrapContext in user space
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        task_control_block
    }

    /// spawn a new process
    pub fn spawn(self: &Arc<Self>, elf_data: &[u8]) -> Arc<Self> {
        // ---- access parent PCB exclusively
        let mut parent_inner = self.inner_exclusive_access();
        // new app data
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    heap_bottom: user_sp,
                    program_brk: user_sp,
                    start_time: 0,
                    syscall_times: [0; MAX_SYSCALL_NUM],
                    stride: Stride(0),
                    priority: 16,
                })
            },
        });
        // add child
        parent_inner.children.push(task_control_block.clone());
        // modify kernel_sp in trap_cx
        // **** access child PCB exclusively
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize,
        );
        // return
        task_control_block
        // **** release child PCB
        // ---- release parent PCB
    }

    /// Load a new elf to replace the original application address space and start execution
    pub fn exec(&self, elf_data: &[u8]) {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();

        // **** access current TCB exclusively
        let mut inner = self.inner_exclusive_access();
        // substitute memory_set
        inner.memory_set = memory_set;
        // update trap_cx ppn
        inner.trap_cx_ppn = trap_cx_ppn;
        // initialize base_size
        inner.base_size = user_sp;
        // initialize trap_cx
        let trap_cx = inner.get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize,
        );
        // **** release inner automatically
    }

    /// parent process fork the child process
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        // ---- access parent PCB exclusively
        let mut parent_inner = self.inner_exclusive_access();
        // copy user space(include trap context)
        let memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: parent_inner.base_size,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    heap_bottom: parent_inner.heap_bottom,
                    program_brk: parent_inner.program_brk,
                    start_time: 0,
                    syscall_times: [0; MAX_SYSCALL_NUM],
                    stride: Stride(0),
                    priority: 16,
                })
            },
        });
        // add child
        parent_inner.children.push(task_control_block.clone());
        // modify kernel_sp in trap_cx
        // **** access child PCB exclusively
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kernel_stack_top;
        // return
        task_control_block
        // **** release child PCB
        // ---- release parent PCB
    }

    /// get pid of process
    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    /// change the location of the program break. return None if failed.
    pub fn change_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner_exclusive_access();
        let heap_bottom = inner.heap_bottom;
        let old_break = inner.program_brk;
        let new_brk = inner.program_brk as isize + size as isize;
        if new_brk < heap_bottom as isize {
            return None;
        }
        let result = if size < 0 {
            inner
                .memory_set
                .shrink_to(VirtAddr(heap_bottom), VirtAddr(new_brk as usize))
        } else {
            inner
                .memory_set
                .append_to(VirtAddr(heap_bottom), VirtAddr(new_brk as usize))
        };
        if result {
            inner.program_brk = new_brk as usize;
            Some(old_break)
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
/// task status: UnInit, Ready, Running, Exited
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Zombie,
}

const BIG_STRIDE: u64 = u64::MAX;

#[derive(Debug, Clone, Copy)]
pub struct Stride(u64);

impl Display for Stride {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialOrd for Stride {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        fn s_cmp(a: u64, b: u64) -> Option<Ordering> {
            // a > b
            let dis = a - b;
            if dis <= BIG_STRIDE / 2 {
                Some(Ordering::Greater)
            } else {
                Some(Ordering::Less)
            }
        }
        let a = self.0;
        let b = other.0;
        if a > b {
            s_cmp(a, b)
        } else {
            s_cmp(b, a).map(|o| o.reverse())
        }
    }
}

impl PartialEq for Stride {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}
