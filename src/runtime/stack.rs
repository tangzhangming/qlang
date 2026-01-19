//! 协程栈管理
//!
//! 实现 2KB 初始栈，按需增长到最大 1MB

use std::alloc::{self, Layout};
use std::ptr::NonNull;

/// 栈溢出错误
#[derive(Debug, Clone)]
pub struct StackOverflow;

impl std::fmt::Display for StackOverflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Stack overflow")
    }
}

impl std::error::Error for StackOverflow {}

/// 协程栈
///
/// 使用连续栈策略，初始分配 2KB，按需倍增到最大 1MB
pub struct Stack {
    /// 栈底（低地址）
    base: NonNull<u8>,
    /// 栈顶指针（当前使用位置）
    sp: usize,
    /// 已使用大小
    size: usize,
    /// 已分配容量
    capacity: usize,
}

impl Stack {
    /// 初始栈大小：2KB
    pub const INITIAL_SIZE: usize = 2 * 1024;
    /// 最大栈大小：1MB
    pub const MAX_SIZE: usize = 1024 * 1024;
    /// 栈对齐：16 字节
    const ALIGNMENT: usize = 16;

    /// 创建新的协程栈
    pub fn new() -> Result<Self, StackOverflow> {
        let layout = Layout::from_size_align(Self::INITIAL_SIZE, Self::ALIGNMENT)
            .map_err(|_| StackOverflow)?;

        let base = unsafe {
            let ptr = alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                return Err(StackOverflow);
            }
            NonNull::new_unchecked(ptr)
        };

        Ok(Self {
            base,
            sp: Self::INITIAL_SIZE,  // 栈从高地址向低地址增长
            size: 0,
            capacity: Self::INITIAL_SIZE,
        })
    }

    /// 创建指定大小的栈
    pub fn with_capacity(capacity: usize) -> Result<Self, StackOverflow> {
        let capacity = capacity.max(Self::INITIAL_SIZE).min(Self::MAX_SIZE);
        let layout = Layout::from_size_align(capacity, Self::ALIGNMENT)
            .map_err(|_| StackOverflow)?;

        let base = unsafe {
            let ptr = alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                return Err(StackOverflow);
            }
            NonNull::new_unchecked(ptr)
        };

        Ok(Self {
            base,
            sp: capacity,
            size: 0,
            capacity,
        })
    }

    /// 获取栈底地址
    #[inline]
    pub fn base(&self) -> *mut u8 {
        self.base.as_ptr()
    }

    /// 获取栈顶地址（初始位置，高地址）
    #[inline]
    pub fn top(&self) -> *mut u8 {
        unsafe { self.base.as_ptr().add(self.capacity) }
    }

    /// 获取当前栈指针
    #[inline]
    pub fn sp(&self) -> usize {
        self.sp
    }

    /// 设置栈指针
    #[inline]
    pub fn set_sp(&mut self, sp: usize) {
        self.sp = sp;
        self.size = self.capacity.saturating_sub(sp);
    }

    /// 获取已使用大小
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    /// 获取已分配容量
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 检查是否需要增长
    #[inline]
    pub fn needs_grow(&self, additional: usize) -> bool {
        self.sp < additional
    }

    /// 增长栈容量
    ///
    /// 将栈容量翻倍，直到满足需求或达到最大限制
    pub fn grow(&mut self) -> Result<(), StackOverflow> {
        let new_capacity = self.capacity.checked_mul(2).ok_or(StackOverflow)?;
        
        if new_capacity > Self::MAX_SIZE {
            return Err(StackOverflow);
        }

        let old_layout = Layout::from_size_align(self.capacity, Self::ALIGNMENT)
            .map_err(|_| StackOverflow)?;
        let new_layout = Layout::from_size_align(new_capacity, Self::ALIGNMENT)
            .map_err(|_| StackOverflow)?;

        let new_base = unsafe {
            let ptr = alloc::realloc(self.base.as_ptr(), old_layout, new_layout.size());
            if ptr.is_null() {
                return Err(StackOverflow);
            }
            NonNull::new_unchecked(ptr)
        };

        // 栈内容需要移动到新的高地址位置
        let offset = new_capacity - self.capacity;
        unsafe {
            // 将旧栈内容移动到新栈的高地址部分
            std::ptr::copy(
                new_base.as_ptr(),
                new_base.as_ptr().add(offset),
                self.capacity,
            );
            // 清零低地址部分
            std::ptr::write_bytes(new_base.as_ptr(), 0, offset);
        }

        self.base = new_base;
        self.sp += offset;
        self.capacity = new_capacity;

        Ok(())
    }

    /// 重置栈（清空但保留内存）
    pub fn reset(&mut self) {
        self.sp = self.capacity;
        self.size = 0;
        // 可选：清零内存
        unsafe {
            std::ptr::write_bytes(self.base.as_ptr(), 0, self.capacity);
        }
    }

    /// 压入数据
    #[inline]
    pub fn push<T: Copy>(&mut self, value: T) -> Result<(), StackOverflow> {
        let size = std::mem::size_of::<T>();
        let align = std::mem::align_of::<T>();

        // 对齐栈指针
        let aligned_sp = (self.sp - size) & !(align - 1);

        if aligned_sp < size {
            // 需要增长
            self.grow()?;
            return self.push(value);
        }

        self.sp = aligned_sp;
        self.size = self.capacity - self.sp;

        unsafe {
            let ptr = self.base.as_ptr().add(self.sp) as *mut T;
            std::ptr::write(ptr, value);
        }

        Ok(())
    }

    /// 弹出数据
    #[inline]
    pub fn pop<T: Copy>(&mut self) -> Option<T> {
        let size = std::mem::size_of::<T>();

        if self.sp + size > self.capacity {
            return None;
        }

        let value = unsafe {
            let ptr = self.base.as_ptr().add(self.sp) as *const T;
            std::ptr::read(ptr)
        };

        self.sp += size;
        self.size = self.capacity - self.sp;

        Some(value)
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self::new().expect("Failed to allocate stack")
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        if let Ok(layout) = Layout::from_size_align(self.capacity, Self::ALIGNMENT) {
            unsafe {
                alloc::dealloc(self.base.as_ptr(), layout);
            }
        }
    }
}

// Stack 不能安全地在线程间共享（因为它包含原始指针）
// 但我们通过 Goroutine 的包装来保证安全性
unsafe impl Send for Stack {}

// ============================================================================
// 栈扫描支持（为 GC 准备）
// ============================================================================

/// 栈帧信息
/// 
/// 记录一个栈帧的布局，用于 GC 扫描
#[derive(Debug, Clone)]
pub struct StackFrameInfo {
    /// 帧基址偏移（相对于栈底）
    pub base_offset: usize,
    /// 帧大小（字节数）
    pub size: usize,
    /// 引用槽位（相对于帧基址的偏移，单位是 Value 大小）
    pub reference_slots: Vec<u16>,
}

/// 栈扫描上下文
/// 
/// 提供给 GC 扫描器的栈状态信息
pub struct StackScanContext {
    /// 栈指针
    pub stack_ptr: *const u8,
    /// 栈容量
    pub capacity: usize,
    /// 栈帧列表（从栈顶到栈底）
    pub frames: Vec<StackFrameInfo>,
}

impl StackScanContext {
    /// 创建新的扫描上下文
    pub fn new(stack: &Stack) -> Self {
        Self {
            stack_ptr: stack.base(),
            capacity: stack.capacity(),
            frames: Vec::new(),
        }
    }
    
    /// 添加帧信息
    pub fn add_frame(&mut self, info: StackFrameInfo) {
        self.frames.push(info);
    }
    
    /// 获取所有引用槽位（用于 GC 根集扫描）
    /// 
    /// 返回所有活跃帧中的引用槽位地址
    pub fn scan_roots<F>(&self, mut callback: F) 
    where
        F: FnMut(*const u8),
    {
        for frame in &self.frames {
            for &slot_offset in &frame.reference_slots {
                // 计算槽位的实际地址
                // 假设每个槽位是 8 字节（Value 的大小）
                let slot_addr = unsafe {
                    self.stack_ptr.add(frame.base_offset + slot_offset as usize * 8)
                };
                callback(slot_addr);
            }
        }
    }
}

/// GC 根集扫描器 trait
/// 
/// VM 和协程可以实现此 trait 来提供根集扫描功能
pub trait GcRootScanner {
    /// 扫描所有 GC 根
    /// 
    /// 回调函数会被每个可能包含引用的位置调用
    fn scan_gc_roots<F>(&self, callback: F) 
    where
        F: FnMut(*const u8);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_new() {
        let stack = Stack::new().unwrap();
        assert_eq!(stack.capacity(), Stack::INITIAL_SIZE);
        assert_eq!(stack.size(), 0);
    }

    #[test]
    fn test_stack_push_pop() {
        let mut stack = Stack::new().unwrap();
        
        stack.push(42u64).unwrap();
        stack.push(100u64).unwrap();
        
        assert_eq!(stack.pop::<u64>(), Some(100));
        assert_eq!(stack.pop::<u64>(), Some(42));
    }

    #[test]
    fn test_stack_grow() {
        let mut stack = Stack::new().unwrap();
        let old_cap = stack.capacity();
        
        stack.grow().unwrap();
        
        assert_eq!(stack.capacity(), old_cap * 2);
    }
}
