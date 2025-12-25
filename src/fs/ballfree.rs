use crate::fs::state::*;
use crate::fs::types::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::ptr;
use std::usize;
impl FileSystem {


/// 分配磁盘空闲块
pub fn balloc(&mut self) -> u32{
    unsafe {
        // 检查空闲块总数
        if self.filsys.s_nfree == 0 {
            eprintln!("\nDisk Full!!! \n");
        }

        // 从空闲栈取第一个空闲块
        let free_block = self.filsys.s_free[self.filsys.s_pfree as usize -1 ];

        // 空闲栈指针到最后一个（组长块），加载新组长块
        if self.filsys.s_pfree == 1 {
            // 从缓冲区池获取页
            let block_id = DATASTART / BLOCKSIZ + free_block as usize;
            let page_ptr = self.buffer_pool_manager.fetch_pg(block_id as i32).unwrap().unwrap();
            
            let mut BLOCK_BUF:[u32; BLOCKSIZ/4] = [0; BLOCKSIZ/4];
            // 拷贝页数据到 block_buf（按字节拷贝）
            ptr::copy_nonoverlapping(
                page_ptr.as_ptr(),
                BLOCK_BUF.as_ptr() as *mut u8,
                BLOCKSIZ
            );

            // 组长块内容入空闲栈
            for i in 0..=NICFREE-1 {
                self.filsys.s_free[i] = BLOCK_BUF[i];
            }
            self.filsys.s_pfree = NICFREE as u16;
        } else {
            // 空闲栈指针减1
            self.filsys.s_pfree -= 1;
        }

        // 更新超级块状态
        self.filsys.s_nfree -= 1;
        self.filsys.s_fmod = SUPDATE;

        free_block
    }
}

/// 回收磁盘块
pub fn bfree(&mut self, block_num: u32)  {
    unsafe {
        // 更新空闲块总数和超级块标志
        self.filsys.s_nfree += 1;
        self.filsys.s_fmod = SUPDATE;

        // 空闲栈满，生成新组长块
        if self.filsys.s_pfree == NICFREE as u16{
            let mut BLOCK_BUF:[u32; BLOCKSIZ/4] = [0; BLOCKSIZ/4];
            // 空闲栈内容拷贝到 block_buf
            for i in 0..=NICFREE {
                BLOCK_BUF[i] = self.filsys.s_free[i];
            }

            // 写入新组长块到磁盘
            let block_id = DATASTART / BLOCKSIZ + block_num as usize;
            let page_ptr = self.buffer_pool_manager.fetch_pg(block_id as i32).unwrap().unwrap();
            ptr::copy_nonoverlapping(
                BLOCK_BUF.as_ptr() as *const u8,
                page_ptr.as_mut_ptr(),
                BLOCKSIZ
            );

            // 更新空闲栈指针和内容
            self.filsys.s_pfree = 1;
            self.filsys.s_free[0] = 1;
            self.filsys.s_free[1] = block_num;
        } else {
            // 空闲栈未满，直接入栈
            self.filsys.s_free[0] += 1;
            self.filsys.s_pfree += 1;
            self.filsys.s_free[self.filsys.s_pfree as usize] = block_num;
        }

    }
}
}