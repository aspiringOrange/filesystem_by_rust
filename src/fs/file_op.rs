use crate::fs::state::*;
use crate::fs::types::*;
use crate::fs::inode::*;
use crate::fs::ballfree::*;
use std::ptr::*;
/// 返回值：1=有权限，0=无权限
impl FileSystem {
pub fn access(&mut self,
    user_id: i32,
    inode1: InodeRef ,          // 用引用替代C裸指针，保证内存安全
    mode: u16
) -> u32 {
    // 根用户直接返回有权限
    if self.user.get(user_id as usize).map_or(false, |u| u.u_uid == ROOT) {
        return 1;
    }
    let inode = inode1.borrow();
    // 匹配操作类型（读/写/执行）
    match mode {
        READ => {
            // 其他用户可读
            if (inode.di_mode & ODIREAD) != 0 {
                1
            }
            // 同组用户可读
            else if (inode.di_mode & GDIREAD) != 0 
                && self.user[user_id as usize].u_gid == inode.di_gid {
                1
            }
            // 所有者可读
            else if (inode.di_mode & UDIREAD) != 0 
                && self.user[user_id as usize].u_uid == inode.di_uid {
                1
            }
            // 无权限
            else {
                0
            }
        }

        WRITE => {
            // 其他用户可写
            if (inode.di_mode & ODIWRITE) != 0 {
                1
            }
            // 同组用户可写
            else if (inode.di_mode & GDIWRITE) != 0 
                && self.user[user_id as usize].u_gid == inode.di_gid {
                1
            }
            // 所有者可写
            else if (inode.di_mode & UDIWRITE) != 0 
                && self.user[user_id as usize].u_uid == inode.di_uid {
                1
            }
            // 无权限
            else {
                0
            }
        }

        EXICUTE => {
            // 其他用户可执行
            if (inode.di_mode & ODIEXICUTE) != 0 {
                1
            }
            // 同组用户可执行
            else if (inode.di_mode & GDIEXICUTE) != 0 
                && self.user[user_id as usize].u_gid == inode.di_gid {
                1
            }
            // 所有者可执行
            else if (inode.di_mode & UDIEXICUTE) != 0 
                && self.user[user_id as usize].u_uid == inode.di_uid {
                1
            }
            // 无权限
            else {
                0
            }
        }

        // 未知操作类型，返回无权限
        _ => 0,
    }
}
}

impl FileSystem {
    /// 关闭指定文件函数
    /// - user_id: 用户ID
    /// - cfd: 用户文件打开表中的位置
    pub unsafe fn close(&mut self,user_id: u32, cfd: i16) {
    
        // 2. 获取系统打开文件表索引
        let node = self.sys_ofile[self.user[user_id as usize].u_ofile[cfd as usize] as usize].f_inode.clone();;
        if node.borrow().i_ino != 0  {
            self.iput(node);
        }
    
        // 4. 引用计数减1
        self.sys_ofile[self.user[user_id as usize].u_ofile[cfd as usize] as usize].f_count-=1;
        // 5. 标记用户文件表项为空闲
        self.user[user_id as usize].u_ofile[cfd as usize] = SYSOPENFILE as u16+ 1;
    }

/// 打开文件函数
/// - user_id: 用户ID
/// - filename: 文件名
/// - openmode: 打开模式（FREAD/FWRITE/FAPPEND等）
/// - 返回值: Ok(系统打开文件表索引) / Err(错误信息)
pub fn aopen(
    &mut self,
    user_id: i32,
    filename: &str,
    openmode: u16,
) -> u16 {
    // 1. 查找文件对应的磁盘inode号
    let dinodeid = self.namei(filename); // 直接获取u32返回值
    if dinodeid == 100 { // 匹配namei未找到时的返回值100
        eprintln!(">file does not existed!");
        return 100;
    }
    // 2. 获取内存inode并校验权限
    let cur_inode: std::rc::Rc<std::cell::RefCell<Inode>> = self.iget(dinodeid);
    let cur_inode1 = cur_inode.clone();
    let cur_inode2 = cur_inode.clone();
    if self.access(user_id, cur_inode, openmode)==0 {
        eprintln!(">Failed to open file due to unqualified authority!");
        self.iput(cur_inode1);
        return 100;
    }

    // 3. 检查文件是否已打开
    for i in 0..SYSOPENFILE {
        if (self.sys_ofile[i].f_count != 0 && self.sys_ofile[i].f_inode.borrow().i_ino == cur_inode1.borrow().i_ino) {
            eprintln!(">File is open already!");
            self.iput(cur_inode1);
            return 100;
        }
    }

    // 4. 分配系统打开文件表项
    let mut sys_ofile_index =0;
    for i in 0..SYSOPENFILE {
        if(self.sys_ofile[i].f_count == 0){

            break;
        }
        sys_ofile_index+=1;
    }
    if(sys_ofile_index==SYSOPENFILE){
        eprintln!(">Too much file open!");
        self.iput(cur_inode1);
        return 100;
    }

    // 5. 初始化系统打开文件表项
    self.sys_ofile[sys_ofile_index].f_inode = cur_inode1.clone();
    self.sys_ofile[sys_ofile_index].f_flag = openmode as u8;
    // 追加模式需同时开启读写
    if (openmode as u8 & FAPPEND) != 0 {
        self.sys_ofile[sys_ofile_index].f_flag = openmode as u8 | FWRITE | FREAD;
    }
    self.sys_ofile[sys_ofile_index].f_count = 1;
    // 设置文件偏移量
    self.sys_ofile[sys_ofile_index].f_off = if (openmode as u8 & FAPPEND) != 0 {
        cur_inode1.borrow().di_size
    } else {
        0
    };

    // 6. 分配用户打开文件表项
    let user_idx = user_id as usize;
    let mut u_ofile_idx = NOFILE;
    for i in 0..NOFILE-1 {
        if(self.user[user_idx].u_ofile[i]==SYSOPENFILE as u16 + 1){
            u_ofile_idx = i;
            break;
        }
    }
    if(u_ofile_idx == NOFILE){
        eprintln!(">Too much file opened by the user!");
        self.sys_ofile[sys_ofile_index].f_count = 0;
        self.iput(cur_inode1);
        return 100;
    }

    // 7. 关联用户表与系统表
    self.user[user_idx].u_ofile[u_ofile_idx] = sys_ofile_index as u16;

    // 8. 写模式且非追加时，释放旧磁盘块并清空文件大小
    if (openmode as u8 & FWRITE) != 0 && (openmode as u8  & FAPPEND) == 0 {
        let mut inode_mut = cur_inode2.borrow_mut();
        let block_count = (inode_mut.di_size / BLOCKSIZ as u32) + 
            if inode_mut.di_size % BLOCKSIZ as u32 != 0 { 1 } else { 0 };
        for i in 0..block_count as usize {
            self.bfree(inode_mut.di_addr[i] as u32);
        }
        inode_mut.di_size = 0;
    }

    sys_ofile_index as u16
}

/// 读文件函数
    /// - cfd: 用户打开文件表项
    /// - buf: 输出缓冲区
    /// - 返回值: 实际读取字节数（0 表示失败）
    pub unsafe fn read(&mut self, cfd: i32, buf: *mut u8) -> u32 {
        // 1. 通过用户表→系统表获取内存inode
        let u_ofile_idx = self.user[self.user_id as usize].u_ofile[cfd as usize];
        if u_ofile_idx < 0 || u_ofile_idx as usize >= self.sys_ofile.len() {
            eprintln!(">invalid file descriptor!");
            return 0;
        }
        let sys_entry = &mut self.sys_ofile[u_ofile_idx as usize];
        
        // 检查读模式
        if (sys_entry.f_flag & FREAD) == 0 {
            eprintln!(">the file is not opened for read!");
            return 0;
        }

        // 获取inode并校验读权限
        let cur_inode = sys_entry.f_inode.clone();
        let cur_inode1 = sys_entry.f_inode.clone();
   
        if self.access(self.user_id as i32, cur_inode, READ)==0 {
            eprintln!(">fail to read file because of no authority!");
            return 0;
        }


        // 计算需要读取的磁盘块数
        let inode_ref = cur_inode1.borrow();
        let block_count = (inode_ref.di_size / BLOCKSIZ as u32) + 
            if inode_ref.di_size % BLOCKSIZ as u32 != 0 { 1 } else { 0 };

        // 逐块读取数据到缓冲区
        for i in 0..block_count as usize {
            if i >= NADDR { break; } // 限制最大块数（对应原代码i>9）
            let block_id = inode_ref.di_addr[i];
            let page_ptr = self.buffer_pool_manager.fetch_pg((DATASTART / BLOCKSIZ + block_id as usize) as i32).unwrap().unwrap();
            
            // 拷贝数据到缓冲区（buf + i*BLOCKSIZ）
            unsafe { std::ptr::copy_nonoverlapping(
                page_ptr.as_ptr(),
                buf.add(i * BLOCKSIZ),
                BLOCKSIZ
            ) };
        }

        inode_ref.di_size 
    }


    /// 写文件函数
    /// - cfd: 用户打开文件表项
    /// - buf: 输入缓冲区
    /// - size: 写入大小
    /// - 返回值: 实际写入字节数
    pub unsafe fn write(&mut self, cfd: i32, buf: *const u8, size: u32) -> u32 {
        // 1. 检查写/追加模式
        let u_ofile_idx = self.user[self.user_id as usize].u_ofile[cfd as usize];
        if u_ofile_idx < 0 || u_ofile_idx as usize >= self.sys_ofile.len() {
            eprintln!(">invalid file descriptor!");
            return 0;
        }
        
        if (self.sys_ofile[u_ofile_idx as usize].f_flag & FWRITE) == 0 && (self.sys_ofile[u_ofile_idx as usize].f_flag & FAPPEND) == 0 {
            eprintln!(">the file is not opened for write!");
            return 0;
        }

        // 2. 获取inode并校验写权限
        let cur_inode= self.sys_ofile[u_ofile_idx as usize].f_inode.clone();
        if self.access(self.user_id as i32, self.sys_ofile[u_ofile_idx as usize].f_inode.clone(), WRITE)==0 {
            eprintln!(">fail to write file because of no authority!");
            return 0;
        }

        let size_adjust = size - if self.sys_ofile[u_ofile_idx as usize].f_inode.borrow().di_size != 0 { 0 } else { 0 };

        // 3. 重写模式（非追加）
        if (self.sys_ofile[u_ofile_idx as usize].f_flag & FAPPEND) == 0 {
            let block = (size / BLOCKSIZ as u32) + if size % BLOCKSIZ as u32 != 0 { 1 } else { 0 };
            let pre_block = (self.sys_ofile[u_ofile_idx as usize].f_inode.borrow().di_size / BLOCKSIZ as u32) + 
                if self.sys_ofile[u_ofile_idx as usize].f_inode.borrow().di_size % BLOCKSIZ as u32 != 0 { 1 } else { 0 };

            // 写入完整块
            for i in 0..(size / BLOCKSIZ as u32) as usize {
                if i > 9 { break; } // 限制最大块数
                // 分配新块（若超出原有块数）
                if i >= pre_block as usize {
                    let mut new_block = self.balloc();
                    while new_block == DISKFULL {
                        new_block = self.balloc();
                    }
                    self.sys_ofile[u_ofile_idx as usize].f_inode.borrow_mut().di_addr[i] = new_block as u16;
                }
                // 写入数据
                let page_ptr = self.buffer_pool_manager.fetch_pg((DATASTART / BLOCKSIZ + self.sys_ofile[u_ofile_idx as usize].f_inode.borrow_mut().di_addr[i] as usize) as i32).unwrap().unwrap();
                unsafe { std::ptr::copy_nonoverlapping(
                    buf.add(i * BLOCKSIZ),
                    page_ptr.as_ptr() as *mut u8,
                    BLOCKSIZ
                ) };
            }

            // 写入剩余数据
            if size % BLOCKSIZ as u32 != 0 {
                let i: usize = (size / BLOCKSIZ as u32) as usize;
                if i <= 9 {
                    // 分配新块（若超出原有块数）
                    if i >= pre_block as usize {
                        let mut new_block = self.balloc();
                        while new_block == DISKFULL {
                            new_block = self.balloc();
                        }
                        self.sys_ofile[u_ofile_idx as usize].f_inode.borrow_mut().di_addr[i] = new_block as u16;
                    }
                    // 写入剩余字节
                    let page_ptr = self.buffer_pool_manager.fetch_pg((DATASTART / BLOCKSIZ + self.sys_ofile[u_ofile_idx as usize].f_inode.borrow_mut().di_addr[i] as usize) as i32).unwrap().unwrap();
                    unsafe { std::ptr::copy_nonoverlapping(
                        buf.add(i * BLOCKSIZ),
                        page_ptr.as_ptr() as *mut u8,
                        (size % BLOCKSIZ as u32) as usize
                    ) };
                }
            }

            // 释放剩余的旧块
            let i_start = (size / BLOCKSIZ as u32) as usize + if size % BLOCKSIZ as u32 != 0 { 1 } else { 0 };
            for i in i_start..pre_block as usize {
                if i > 9 { break; }
                self.bfree(cur_inode.borrow().di_addr[i] as u32);
            }
        } 
        // 4. 追加模式
        else {
            let block = (size / BLOCKSIZ as u32) + if size % BLOCKSIZ as u32 != 0 { 1 } else { 0 };
            let mut pre_block = (self.sys_ofile[u_ofile_idx as usize].f_inode.borrow().di_size / BLOCKSIZ as u32) + 
                if self.sys_ofile[u_ofile_idx as usize].f_inode.borrow().di_size % BLOCKSIZ as u32 != 0 { 1 } else { 0 };
            let mut buf_off = 0;

            // 处理最后一块续写
            if pre_block == 1 {
                pre_block = 0;
            } else if pre_block == 0 {
                // 分配第一个块
                let mut new_block = self.balloc();
                while new_block == DISKFULL {
                    new_block = self.balloc();
                }
                self.sys_ofile[u_ofile_idx as usize].f_inode.borrow_mut().di_addr[pre_block as usize] = new_block as u16;
            }

            // 写入最后一块的剩余空间
            let write_len = if size < (BLOCKSIZ as u32 - self.sys_ofile[u_ofile_idx as usize].f_inode.borrow().di_size) {
                size
            } else {
                BLOCKSIZ as u32 - self.sys_ofile[u_ofile_idx as usize].f_inode.borrow().di_size
            } as usize;
            let page_ptr = self.buffer_pool_manager.fetch_pg((DATASTART / BLOCKSIZ + self.sys_ofile[u_ofile_idx as usize].f_inode.borrow_mut().di_addr[pre_block as usize] as usize) as i32).unwrap().unwrap();
            let offset = if self.sys_ofile[u_ofile_idx as usize].f_inode.borrow().di_size != 0 {
                self.sys_ofile[u_ofile_idx as usize].f_inode.borrow().di_size % BLOCKSIZ as u32 -2
            } else {
                0
            } as usize;
            unsafe { std::ptr::copy_nonoverlapping(
                buf,
                (page_ptr.as_ptr() as *mut u8).add(offset),
                write_len
            ) };
            buf_off += write_len;

            // 写入剩余数据（新块）
            if buf_off < size as usize {
                let remaining = size as usize - buf_off;
                let block_cnt = remaining / BLOCKSIZ;
                let i =0;
                // 写入完整新块
                for i in 0..block_cnt {
                    pre_block += 1;
                    if pre_block > 9 { break; }
                    // 分配新块
                    let mut new_block = self.balloc();
                    while new_block == DISKFULL {
                        new_block = self.balloc();
                    }
                    self.sys_ofile[u_ofile_idx as usize].f_inode.borrow_mut().di_addr[pre_block as usize] = new_block as u16;
                    // 写入数据
                    let page_ptr = self.buffer_pool_manager.fetch_pg((DATASTART / BLOCKSIZ + self.sys_ofile[u_ofile_idx as usize].f_inode.borrow().di_addr[pre_block as usize] as usize) as i32).unwrap().unwrap();
                    unsafe { std::ptr::copy_nonoverlapping(
                        buf.add(buf_off + i * BLOCKSIZ),
                        page_ptr.as_ptr() as *mut u8,
                        BLOCKSIZ
                    ) };
                }

                // 写入最后一块剩余数据
                if remaining % BLOCKSIZ != 0 {
                    pre_block += 1;
                    if pre_block <= 9 {
                        // 分配新块
                        let mut new_block = self.balloc();
                        while new_block == DISKFULL {
                            new_block = self.balloc();
                        }
                        self.sys_ofile[u_ofile_idx as usize].f_inode.borrow_mut().di_addr[pre_block as usize] = new_block as u16;
                        // 写入剩余字节
                        let page_ptr = self.buffer_pool_manager.fetch_pg((DATASTART / BLOCKSIZ + self.sys_ofile[u_ofile_idx as usize].f_inode.borrow_mut().di_addr[pre_block as usize] as usize) as i32).unwrap().unwrap();
                        unsafe { std::ptr::copy_nonoverlapping(
                            buf.add(buf_off + i * BLOCKSIZ),
                            page_ptr.as_ptr() as *mut u8,
                            remaining % BLOCKSIZ
                        );
                        }
                    }
                }
            }
        }
        // 5. 更新偏移量和文件大小
        self.sys_ofile[u_ofile_idx as usize].f_off += size_adjust;
        cur_inode.borrow_mut().di_size += size_adjust;
        //println!("cur_inode.borrow_mut().di_size{}",cur_inode.borrow_mut().di_size);

        // 限制最大大小（10*BLOCKSIZ）
        let max_size = (10 * BLOCKSIZ) as u32;
        if cur_inode.borrow_mut().di_size > max_size {
            cur_inode.borrow_mut().di_size = max_size;
        }
        if self.sys_ofile[u_ofile_idx as usize].f_off > max_size {
            self.sys_ofile[u_ofile_idx as usize].f_off = max_size;
        }

        size
    }

    /// 检查文件是否已打开
    pub fn xfa(&mut self, filename: &str) -> i32 {
        // 1. 调用namei查找inode编号
        let ino = self.namei(filename);
        if ino == 100 {
            return -1;
        }

        // 2. 遍历用户打开文件表
        let mut i = 0;
        while i < NOFILE {
            if self.user[self.user_id as usize].u_ofile[i] == SYSOPENFILE as u16 + 1 {
                // 空分支跳过
            } else {
                let t = self.user[self.user_id as usize].u_ofile[i];
                if t < 0 || t as usize >= self.sys_ofile.len() {
                    i += 1;
                    continue;
                }
                let sys_inode = self.sys_ofile[t as usize].f_inode.borrow();
                if sys_inode.i_ino == ino {
                    break;
                }
            }
            i += 1;
        }

        // 3. 返回逻辑
        if i != NOFILE {
            i as i32
        } else {
            -1
        }
    }

    pub fn cpy(&mut self, filename: &str, cache: &mut [u8], temp_file_size: &mut i32) {
        // 1. 调用namei
        let cpdino = self.namei(filename);

        // 2. 权限校验
        let newinode = self.iget(cpdino);
        if self.access(self.user_id as i32, newinode.clone(), READ)==0 {
            eprintln!("\n对不起，您没有复制该文件的权限！\n");
            self.iput(newinode);
            return;
        }
        self.iput(newinode);


        // 3. 调用xfa
        let tfd = self.xfa(filename);
        if tfd != -1 {
            let sys_otpos = self.user[self.user_id as usize].u_ofile[tfd as usize];
            *temp_file_size = self.sys_ofile[sys_otpos as usize].f_inode.borrow().di_size as i32;

            // 读文件内容
            unsafe {
                self.read(tfd, cache.as_mut_ptr());
            }

            // 输出内容
            let content = unsafe { core::str::from_utf8_unchecked(cache) };
            eprintln!("\n>content you want to copy: {}\n", content);
        } else {
            eprintln!(">Cannot read a file that is not open or not exixted in current directory");
            return;
        }

        unsafe { self.close(self.user_id as u32, tfd as i16) };
    }

    pub fn pst(&mut self, filename: &str, cache: &[u8], temp_file_size: i32) {
        // 1. 调用namei
        let cpdino = self.namei(filename);

        // 2. 权限校验
        let newinode = self.iget(cpdino);
        if self.access(self.user_id as i32, newinode.clone(), READ)==0 {
            eprintln!("\n对不起，您没有复制该文件的权限！\n");
            self.iput(newinode);
            return;
        }
        self.iput(newinode);

        // 3. 调用xfa
        let tfd = self.xfa(filename);
        if tfd != -1 {
            unsafe {
                self.write(tfd, cache.as_ptr(), temp_file_size as u32);
            }
        } else {
            eprintln!(">Cannot write a file that is not open or not exixted in current directory");
        }

        unsafe { self.close(self.user_id as u32, tfd as i16) };
    }


}