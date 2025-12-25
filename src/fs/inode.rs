use crate::fs::state::*;
use crate::fs::types::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::ptr;
use std::usize;
impl FileSystem {
    /// =========================
    /// iget：获取内存 i 节点（可能需要从磁盘读取）
    /// param 磁盘 i 节点 id
    /// return：内存 i 节点句柄
    /// =========================
    pub fn iget(&mut self, dinodeid: u32) -> InodeRef {
        let bucket = (dinodeid as usize) % NHINO;

        // 1) 先在哈希桶里找是否已存在
        if let Some(found) = self.hinode[bucket]
        .iter()
        // 改进：先拷贝 i_ino 值，再比较，borrow() 的生命周期仅在这一行
        .find(|n| {
            let ino = n.borrow().i_ino; // borrow() 仅在这一行有效，立即释放
            ino == dinodeid
        })
        .cloned()
        {
            found.borrow_mut().i_count += 1; 
            return found;
        }
        //println!("{}",dinodeid);
        // 2) 不存在：从磁盘读 dinode，构造新的 MemInode
        let din = self.read_dinode(dinodeid);

        let newinode = Rc::new(RefCell::new(Inode {
            i_flag: 0,        // not update
            i_ino: dinodeid,  // 标识内存 i 节点
            i_count: 1,       // 引用计数

            di_number: din.di_number,
            di_mode: din.di_mode,
            di_uid: din.di_uid,
            di_gid: din.di_gid,
            di_size: din.di_size,
            di_addr: din.di_addr,
        }));
        //println!("iget {} {}",dinodeid,din.di_size);
        // 3) 放入对应桶
        self.hinode[bucket].push(newinode.clone());

        newinode
    }

    /// =========================
    /// iput：释放内存 i 节点
    /// param 内存 i 节点句柄
    /// =========================
    pub fn iput(&mut self, inode: InodeRef) {
        // 先拿到 ino 和 bucket，后面要从缓存移除
        let (ino, bucket, need_writeback, di_number, di_size, di_addr) = {
            let n = inode.borrow();

            let bucket = (n.i_ino as usize) % NHINO;
            let ino = n.i_ino;
            //println!("{} {}",ino,n.i_count);
            // 若引用计数>=2，只减 1 返回
            if n.i_count > 1 {
                drop(n);
                inode.borrow_mut().i_count -= 1;
                return;
            }

            // 引用计数=1：需要最终回收
            // 若文件关联计数!=0 => 写回 dinode
            let need_writeback = n.di_number != 0;

            (ino, bucket, need_writeback, n.di_number, n.di_size, n.di_addr)
        };

        if need_writeback {
            //println!("need_writeback");
            // 将 dinode 部分写回磁盘
           // println!("need_writeback {} {}",ino,inode.borrow().di_size);
            self.write_dinode(ino, di_number, inode.borrow().di_mode, inode.borrow().di_uid,
                              inode.borrow().di_gid, inode.borrow().di_size, inode.borrow().di_addr);
        } else {
            // 删除文件：释放磁盘块 + 释放磁盘 i 节点
            let blocks = (di_size as usize + BLOCKSIZ - 1) / BLOCKSIZ;
            for i in 0..blocks {
                self.bfree(di_addr[i] as u32);
            }
            self.ifree(ino);
        }

        // 从哈希桶移除
        let vec = &mut self.hinode[bucket];
        if let Some(pos) = vec.iter().position(|x| Rc::ptr_eq(x, &inode)) {
            vec.swap_remove(pos);
        }
        // inode 离开缓存后，Rc 计数归零就会被自动释放（无需 free）
    }



    fn read_dinode(&mut self, dinodeid: u32) -> Dinode {
        debug_assert_eq!(std::mem::size_of::<Dinode>(), DINODESIZ);

        // 1. 计算块号（修正：全部用 usize，避免 i32 类型）
        let entries_per_block = BLOCKSIZ / DINODESIZ;
        let block_idx: usize = DINODESTART / BLOCKSIZ + (dinodeid as usize) / entries_per_block;
        
        // 2. 获取缓冲区（修正：优雅处理 Result/Option，避免 unwrap panic）
        let page_ptr = self.buffer_pool_manager.fetch_pg(block_idx as i32).unwrap(); // 若 fetch_pg 必须 i32，显式转换
        let page_buf = page_ptr.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "缓冲区为空")
        }).unwrap();

        // 3. 计算页内偏移（修正：转 usize，校验越界）
        let inode_offset_in_block = (dinodeid as usize) % entries_per_block;
        let byte_offset = inode_offset_in_block * DINODESIZ;
        

        // 4. 初始化 Dinode（默认值）
        let mut din = Dinode {
            di_number: 0,
            di_mode: 0,
            di_uid: 0,
            di_gid: 0,
            di_size: 0,
            di_addr: [0u16; NADDR],
        };

        // 5. 安全的内存拷贝（修正 unsafe 逻辑）
        unsafe {
            // 步骤1：获取缓冲区起始指针
            let src_ptr = page_buf.as_ptr();
            // let bytes = unsafe { std::slice::from_raw_parts(src_ptr, 100) };

            // // 一字节一字节打印（无任何额外逻辑）
            // for &byte in bytes {
            //     print!("{} ", byte); // 十进制输出，字节间仅空格分隔
            // }
  

            // 步骤2：计算偏移后的指针（用 offset，必须是 isize）
            let src_ptr_offset = src_ptr.offset(byte_offset as isize);
            // 步骤3：转换为 Dinode 类型指针
            let src_dinode_ptr = src_ptr_offset as *const Dinode;
            // 步骤4：获取目标指针（&mut din 转裸指针）
            let dest_dinode_ptr = &mut din as *mut Dinode;
            // 步骤5：内存拷贝（1 个 Dinode 实例）
            ptr::copy_nonoverlapping(src_dinode_ptr, dest_dinode_ptr, 1);

            //println!("{} block_idx{} offset{}  din.di_number{}",dinodeid,block_idx,byte_offset,din.di_number);
        }

        din
    }

    fn write_dinode(&mut self, ino: u32, di_number: u16, di_mode: u16, di_uid: u16, di_gid: u16, di_size: u32, di_addr: [u16; NADDR]) {
        debug_assert_eq!(std::mem::size_of::<Dinode>(), DINODESIZ);

        debug_assert_eq!(std::mem::size_of::<Dinode>(), DINODESIZ);

        // 1. 计算块号（修正：全部用 usize，避免 i32 类型）
        let entries_per_block = BLOCKSIZ / DINODESIZ;
        let block_idx: usize = DINODESTART / BLOCKSIZ + (ino as usize) / entries_per_block;
        
        // 2. 获取缓冲区（修正：优雅处理 Result/Option，避免 unwrap panic）
        let page_ptr = self.buffer_pool_manager.fetch_pg(block_idx as i32).unwrap(); // 若 fetch_pg 必须 i32，显式转换
        let page_buf = page_ptr.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "缓冲区为空")
        }).unwrap();

        // 3. 计算页内偏移（修正：转 usize，校验越界）
        let inode_offset_in_block = (ino as usize) % entries_per_block;
        let byte_offset = inode_offset_in_block * DINODESIZ;

        let din = Dinode { di_number, di_mode, di_uid, di_gid, di_size, di_addr };

        // 5. 安全的内存拷贝（修正 unsafe 逻辑）
        unsafe {
            // 步骤1：获取缓冲区起始指针
            let src_ptr = page_buf.as_ptr();
            // 步骤2：计算偏移后的指针（用 offset，必须是 isize）
            let src_ptr_offset = src_ptr.offset(byte_offset as isize);
            // 步骤3：转换为 Dinode 类型指针
            let src_dinode_ptr = src_ptr_offset as *mut Dinode;
            // 步骤4：获取目标指针（&mut din 转裸指针）
            let dest_dinode_ptr = &din as *const Dinode;
            // 步骤5：内存拷贝（1 个 Dinode 实例）
            ptr::copy_nonoverlapping(dest_dinode_ptr, src_dinode_ptr,1);
        }
    }
    

    pub fn ialloc(&mut self) -> InodeRef {
        unsafe {
            // 检查空闲 inode 总数
            if self.filsys.s_ninode == 0 {
                eprintln!(">Inode null!");
            }
    
            // 空闲 inode 栈耗尽，重新加载
            if self.filsys.s_pinode == NICINOD as u16 {
                let mut i = 0;
                let mut count = 0;
                let mut block_end_flag = 1;
                self.filsys.s_pinode = NICINOD as u16 - 1;
                let mut cur_di = self.filsys.s_rinode;
                let mut BLOCK_BUF:[Dinode; BLOCKSIZ / DINODESIZ] = [Dinode::default(); BLOCKSIZ / DINODESIZ];
                // 加载新的空闲 inode 到栈
                while count <= NICINOD && count <= self.filsys.s_ninode as usize{
                    if block_end_flag ==1 {
                        // 计算块号 + 读取缓冲区
                        let block_id = DINODESTART / BLOCKSIZ + (cur_di / (BLOCKSIZ / DINODESIZ) as u32) as usize;
                        if cur_di <= ((BLOCKSIZ * (DINODEBLK - 1)) / DINODESIZ) as u32 {
                            // 读取跨块数据
                            let page_ptr1 = self.buffer_pool_manager.fetch_pg(block_id as i32).unwrap().unwrap();
                            let offset1 = (cur_di % (BLOCKSIZ / DINODESIZ) as u32) as usize * DINODESIZ;
                            let copy_len1 = DINODESIZ - (cur_di % (BLOCKSIZ / DINODESIZ) as u32) as usize * DINODESIZ / DINODESIZ;
                            ptr::copy_nonoverlapping(
                                page_ptr1.as_ptr().add(offset1),
                                BLOCK_BUF.as_mut_ptr() as *mut u8,
                                copy_len1 * DINODESIZ
                            );
    
                            let page_ptr2 = self.buffer_pool_manager.fetch_pg(block_id as i32 +1).unwrap().unwrap();
                            let copy_len2 = (cur_di % (BLOCKSIZ / DINODESIZ) as u32) as usize;
                            ptr::copy_nonoverlapping(
                                page_ptr2.as_ptr(),
                                BLOCK_BUF.as_mut_ptr().add(copy_len1) as *mut u8,
                                copy_len2 * DINODESIZ
                            );
                            block_end_flag = 0;
                            i = 0;
                        } else {
                            // 读取剩余数据
                            let page_ptr = self.buffer_pool_manager.fetch_pg(block_id as i32).unwrap().unwrap();
                            let offset = (cur_di % (BLOCKSIZ / DINODESIZ) as u32) as usize * DINODESIZ;
                            ptr::copy_nonoverlapping(
                                page_ptr.as_ptr().add(offset),
                                BLOCK_BUF.as_mut_ptr() as *mut u8,
                                (BLOCKSIZ * DINODEBLK - cur_di as usize * DINODESIZ)
                            );
                            block_end_flag = 0;
                            i = 0;
                        }
                    }
    
                    // 查找空闲 inode
                    while BLOCK_BUF[i].di_mode != DIEMPTY && i < BLOCKSIZ / DINODESIZ {
                        cur_di += 1;
                        i += 1;
                    }
    
                    // 处理块遍历完成/找到空闲 inode
                    if i == BLOCKSIZ / DINODESIZ {
                        block_end_flag = 1;
                    } else {
                        if count != NICINOD {
                            self.filsys.s_inode[self.filsys.s_pinode as usize] = cur_di;
                            self.filsys.s_pinode -= 1;
                            count += 1;
                            cur_di += 1;
                            i += 1;
                        } else {
                            count += 1;
                        }
                    }
                }
    
                // 更新超级块
                self.filsys.s_rinode = cur_di;
                self.filsys.s_pinode += 1;
            }
    
            // 分配 inode 并更新超级块
            let dinode_id = self.filsys.s_inode[self.filsys.s_pinode as usize];
            //println!("{}",dinode_id);
            let mut temp_inode = self.iget(dinode_id);
        
    
            // 更新超级块状态
            self.filsys.s_pinode += 1;
            self.filsys.s_ninode -= 1;
            self.filsys.s_fmod = SUPDATE;
    
            temp_inode
        }
    }

    /// 回收磁盘 inode
    pub fn ifree(&mut self, dinodeid: u32) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            // 更新空闲 inode 计数
            self.filsys.s_ninode += 1;

            // 空闲栈未满，直接入栈
            if  self.filsys.s_pinode != 0 {
                self.filsys.s_pinode -= 1;
                self.filsys.s_inode[ self.filsys.s_pinode as usize] = dinodeid;
            } else if dinodeid <  self.filsys.s_rinode {
                // 更新铭记 inode
                self.filsys.s_rinode = dinodeid;
            }

            // 标记 inode 为空闲并写回磁盘
            let mut BLOCK_BUF:[Dinode; BLOCKSIZ / DINODESIZ] = [Dinode::default(); BLOCKSIZ / DINODESIZ];
            BLOCK_BUF[0].di_mode = DIEMPTY;
            let block_id = DINODESTART / BLOCKSIZ + (dinodeid / (BLOCKSIZ / DINODESIZ) as u32) as usize;
            let page_ptr = self.buffer_pool_manager.fetch_pg(block_id as i32).unwrap().unwrap();
            let offset = (dinodeid % (BLOCKSIZ / DINODESIZ) as u32) as usize * DINODESIZ;
            
            ptr::copy_nonoverlapping(
                &BLOCK_BUF[0].di_number as *const u16,
                page_ptr.as_mut_ptr().add(offset) as *mut u16,
                DINODESIZ / 2 // 按 u16 元素数拷贝
            );

            Ok(())
        }
    }

}