use crate::fs::state::*;
use crate::fs::types::*;
use crate::fs::cli::*;
use crate::fs::inode::*;
use crate::fs::ballfree::*;
use crate::fs::dir_op::*;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr::*;
/// 返回值：1=有权限，0=无权限
impl FileSystem {

    pub unsafe fn login(&mut self, username: *const c_char, passwd: *const c_char) -> i32 {
        // 空指针校验
        if username.is_null() || passwd.is_null() {
            eprintln!(">Invalid username/password!");
            return -1;
        }
        //println!("1");
        // 1. 转换C字符串为Rust字符串
        let username_str = CStr::from_ptr(username)
            .to_str()
            .unwrap_or_default();
        let passwd_str = CStr::from_ptr(passwd)
            .to_str()
            .unwrap_or_default();
        //println!("2");
        // 2. 查找用户名匹配的密码表项
        let mut i = 0;
        while i < PWDNUM {
            let pwd_username = CStr::from_bytes_until_nul(&self.pwd[i].username)
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();
            if pwd_username == username_str {
                break;
            }
            i += 1;
        }
        //println!("3");
        // 用户名不匹配
        if i == PWDNUM {
            eprintln!(">Incorrect username. please retry");
            return -1;
        }

        // 3. 校验密码
        let mut tempuuid = 0u16;
        let mut tempgid = 0u16;
        let pwd_passwd = CStr::from_bytes_until_nul(&self.pwd[i].password)
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default();
        if pwd_passwd == passwd_str {
            tempuuid = self.pwd[i].p_uid;
            tempgid = self.pwd[i].p_gid;
        } else {
            eprintln!(">Incorrect password. please retry");
            return -1;
        }
        //println!("4");
        // 4. 查找空闲用户表项
        let mut j = 0;
        while j < USERNUM {
            if self.user[j].u_uid == 0 { // 空闲表项（u_uid=0）
                // 填充用户信息
                self.user[j].u_uid = tempuuid;
                self.user[j].u_gid = tempgid;
                // 设置默认权限模式（根用户=0，普通用户=其他）
                self.user[j].u_default_mode = if i == 0 { ROOTMODE } else { USERMODE };
                break;
            }
            j += 1;
        }

        // 用户表满
        if j == USERNUM {
            eprintln!(">Too many users. Please wait");
            return -1;
        }
        //println!("5");
        //println!("{}",i);
        // 成功返回用户表索引
        j as i32
    }

    pub unsafe fn logout(&mut self, uid: u16) -> i32 {
        // 1. 查找匹配的用户表项
        let mut i = 0;
        while i < USERNUM {
            if self.user[i].u_uid == uid {
                break;
            }
            i += 1;
        }

        // 未找到用户
        if i == USERNUM {
            eprintln!(">Cannot find the user you want to log out");
            return 0; 
        }

        // 2. 清空用户打开文件表，释放关联资源
        for j in 0..NOFILE {
            if self.user[i].u_ofile[j] != SYSOPENFILE as u16 +1 {
                // 获取系统打开文件表索引
                let sys_no = self.user[i].u_ofile[j] as usize;

                // 释放inode、减少引用计数
                self.iput(self.sys_ofile[sys_no].f_inode.clone()); // 释放内存inode

                self.sys_ofile[sys_no].f_count-=1;
                // 重置用户文件表项为空闲值
                self.user[i].u_ofile[j] = SYSOPENFILE as u16 +1;
            }
        }

        // 3. 清空用户表项
        self.user[i].u_default_mode = DEFAULTMODE;
        self.user[i].u_gid = 0;
        self.user[i].u_uid = 0;

        // 4. 清空当前用户ID，切回根目录
        self.user_id = -1;
        // 循环跳转到根目录（cur_path_inode->i_ino == 1）
        while self.cur_path_inode.borrow().i_ino != 1 {
            self.chdir(ROOT as i32, ".."); 
        }
        1
    }
    /// 退出文件系统函数
    pub unsafe fn halt(&mut self) {
        // ========== Step1: 清空所有用户相关记录 ==========
        // 遍历所有用户，关闭所有打开文件
        for i in 0..USERNUM {
            if self.user[i].u_uid != 0 { // 用户已登录
                for j in 0..NOFILE {
                    if self.user[i].u_ofile[j] != SYSOPENFILE as u16  + 1 { // 文件表项有效
                        // 关闭文件（复用之前实现的close函数）
                        self.close(i as u32 , j as i16);
                        // 标记文件表项为空闲
                        self.user[i].u_ofile[j] = SYSOPENFILE as u16 + 1;
                    }
                }
            }
        }

        // ========== Step2: 处理所有内存inode ==========
        // 遍历每个哈希桶（Vec）
        let mut inodes: Vec<InodeRef> = Vec::new();
        // 仅不可变借用hinode，遍历并拷贝所有inode
        for bucket in &self.hinode {
            for inode_rc in bucket {
                // 直接拷贝整个inode到临时Vec（前提：Inode实现Copy/Clone）
                inodes.push(inode_rc.clone()); 
            }
        }

        for inode_rc in inodes {
            let mut inode_mut = inode_rc.borrow_mut();
            //println!("{} {}",inode_mut.i_ino,inode_mut.di_size);
            if inode_mut.di_number != 0 {
                // 关联计数非0：将inode写回磁盘（原C版逻辑）
                let ino = inode_mut.i_ino;
                //println!("{} {}",inode_mut.i_ino,inode_mut.di_size);
                // 计算inode所在磁盘块和偏移
                let block_id = (DINODESTART / BLOCKSIZ + ino as usize / (BLOCKSIZ / DINODESIZ) as usize) as i32;
                let offset = (ino % (BLOCKSIZ / DINODESIZ) as u32) as usize * DINODESIZ;
                // 获取缓冲区页并拷贝数据
                let page_ptr = self.buffer_pool_manager.fetch_pg(block_id).unwrap().unwrap();
                let src_ptr = (&inode_mut.di_number as *const u16).cast::<u8>();
                unsafe { std::ptr::copy_nonoverlapping(
                    src_ptr,
                    (page_ptr.as_ptr() as *mut u8).add(offset),
                    DINODESIZ
                ) };
            } else {
                // 关联计数为0：释放磁盘块和inode
                let block_num = (inode_mut.di_size / BLOCKSIZ as u32) + 
                    if inode_mut.di_size % BLOCKSIZ as u32 != 0 { 1 } else { 0 };
                // 释放所有磁盘块
                for j in 0..block_num as usize {
                    if j < inode_mut.di_addr.len() {
                        self.bfree(inode_mut.di_addr[j] as u32);
                    }
                }
                // 释放磁盘inode
                self.ifree(inode_mut.i_ino);
            }
        }

        

        // ========== Step3: 写回超级块并关闭文件系统 ==========
        // 写回超级块到磁盘块1
        let page_ptr = self.buffer_pool_manager.fetch_pg(1).unwrap().unwrap();
        unsafe { std::ptr::copy_nonoverlapping(&self.filsys, page_ptr.as_ptr() as *mut SuperBlock, 1) };
        // 刷新所有缓冲区到磁盘
        self.buffer_pool_manager.flush_all_pgs();

        // 退出程序
        // eprintln!(">Good Bye!See You Next Time.");
        // std::process::exit(0);
    }

}