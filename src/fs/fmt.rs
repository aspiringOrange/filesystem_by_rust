use std::cell::RefCell;
use std::rc::Rc;
use std::ptr;
use crate::fs::types::*;
use crate::fs::bufferpool::*;
pub type InodeRef = Rc<RefCell<Inode>>;
use crate::fs::inode::*;
use crate::fs::state::*;
use std::io::{self, Read, Seek, SeekFrom, Write};
impl FileSystem {
pub fn copy_str_to_bytes(&mut self,s: &str, dest: &mut [u8]) {
        let bytes = s.as_bytes();
        let len = bytes.len().min(dest.len());
        dest[..len].copy_from_slice(&bytes[..len]);
        // 剩余位置填 0
        for i in len..dest.len() {
            dest[i] = 0;
        }
    }

pub fn format(&mut self,disk_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // ========== 1. 初始化磁盘文件 ==========
    // 创建并清空磁盘文件
    // if Path::new(disk_path).exists() {
    //     let _ = fs::remove_file(disk_path);
    // }
    let mut init_file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(disk_path)?;
    // 分配并写入空缓冲区
    let empty_buf = [0u8; (DINODEBLK + FILEBLK + 2) * BLOCKSIZ];
    init_file.seek(SeekFrom::Start(0));
    init_file.write_all(&empty_buf)?;
    init_file.flush()?; 
    
    //drop(init_file); // 释放文件句柄

    // 以读写模式重新打开磁盘
    // let mut fd = std::fs::OpenOptions::new()
    //     .read(true)
    //     .write(true)
    //     .open(disk_path)?;

    // ========== 2. 初始化 inode 区域 ==========
    let inodes_per_block = BLOCKSIZ / DINODESIZ;
    let mut block_buff = [Dinode::default(); BLOCKSIZ / DINODESIZ]; // 单块 inode 缓冲区
    //self.buffer_pool_manager = BufferPoolManager::new(1000,5, disk_path).unwrap();
    for i in 0..DINODEBLK {
        // 从缓冲区池获取页
        let page_ptr = self.buffer_pool_manager.fetch_pg((DINODESTART/BLOCKSIZ + i) as i32).unwrap().unwrap();
        // 读取页数据到缓冲区
        unsafe { ptr::copy_nonoverlapping(page_ptr as *mut u8, block_buff.as_mut_ptr() as *mut u8, BLOCKSIZ) };
        // 置空所有 inode 模式
        for j in 0..inodes_per_block {
            block_buff[j].di_mode = DIEMPTY;
        }
        // 写回缓冲区池
        unsafe { ptr::copy_nonoverlapping(block_buff.as_ptr() as *mut u8, page_ptr as *mut u8, BLOCKSIZ) };
    }

    // ========== 3. 初始化密码缓冲区 ==========
    let mut passwd = [Password::default(); BLOCKSIZ / (PWDSIZ * 2 + 4)];
    // 初始化 root 密码
    passwd[0].p_uid = ROOT;
    passwd[0].p_gid = ROOT;
    self.copy_str_to_bytes("root", &mut passwd[0].username);
    self.copy_str_to_bytes("root", &mut passwd[0].password);

    // ========== 4. 初始化 inode 0（空节点） ==========
    let mut inode = self.iget(0);
    inode.borrow_mut().di_mode = DIEMPTY;
    self.iput(inode);

    // ========== 5. 初始化根目录（inode 1） ==========
    let mut dir_buf = [DirEntry::default(); BLOCKSIZ / (DIRSIZ + 2)];
    let mut inode = self.iget(1);
    inode.borrow_mut().di_number = 1;
    inode.borrow_mut().di_mode = ROOTMODE | DIDIR;
    inode.borrow_mut().di_size = (3 * (DIRSIZ + 2)) as u32;
    inode.borrow_mut().di_addr[0] = 0;
    // 填充根目录项
    self.copy_str_to_bytes(".", &mut dir_buf[0].d_name);
    dir_buf[0].d_ino = 1;
    self.copy_str_to_bytes("..", &mut dir_buf[1].d_name);
    dir_buf[1].d_ino = 1;
    self.copy_str_to_bytes("etc", &mut dir_buf[2].d_name);
    dir_buf[2].d_ino = 2;
    // 写入数据块 0
    let page_ptr = self.buffer_pool_manager.fetch_pg((DATASTART/BLOCKSIZ) as i32).unwrap().unwrap();
    unsafe { ptr::copy_nonoverlapping(dir_buf.as_ptr() as *mut u8, page_ptr as *mut u8, BLOCKSIZ) };
    self.iput(inode);
    let mut inode = self.iget(1);
    //println!("{}",inode.borrow_mut().di_size);
    self.iput(inode);

    // ========== 6. 初始化 etc 目录（inode 2） ==========
    let mut inode = self.iget(2);
    inode.borrow_mut().di_number = 1;
    inode.borrow_mut().di_mode = ROOTMODE | DIDIR;
    inode.borrow_mut().di_size = (3 * (DIRSIZ + 2)) as u32;
    inode.borrow_mut().di_addr[0] = 1;
    // 填充 etc 目录项
    self.copy_str_to_bytes(".", &mut dir_buf[0].d_name);
    dir_buf[0].d_ino = 2;
    self.copy_str_to_bytes("..", &mut dir_buf[1].d_name);
    dir_buf[1].d_ino = 1;
    self.copy_str_to_bytes("password", &mut dir_buf[2].d_name);
    dir_buf[2].d_ino = 3;
    // 写入数据块 1
    let page_ptr = self.buffer_pool_manager.fetch_pg((DATASTART/BLOCKSIZ + 1) as i32).unwrap().unwrap();
    unsafe { ptr::copy_nonoverlapping(dir_buf.as_ptr() as *mut u8, page_ptr as *mut u8, BLOCKSIZ) };
    self.iput(inode);
    // ========== 7. 初始化 password 文件（inode 3） ==========
    let mut inode = self.iget(3);
    inode.borrow_mut().di_number = 1;
    inode.borrow_mut().di_mode = ROOTMODE | DIFILE;
    inode.borrow_mut().di_size = (PWDNUM * (2 * PWDSIZ + 4)) as u32;
    inode.borrow_mut().di_addr[0] = 2;
    // 初始化非 root 密码项
    for i in 1..BLOCKSIZ / (PWDSIZ * 2 + 4) {
        passwd[i].p_uid = 0;
        passwd[i].p_gid = 0;
        self.copy_str_to_bytes(" ", &mut passwd[i].username);
        self.copy_str_to_bytes(" ", &mut passwd[i].password);
    }
    // 写入数据块 2
    let page_ptr = self.buffer_pool_manager.fetch_pg((DATASTART/BLOCKSIZ + 2) as i32).unwrap().unwrap();
    unsafe { ptr::copy_nonoverlapping(passwd.as_ptr() as *mut u8, page_ptr as *mut u8, BLOCKSIZ) };
    self.iput(inode);
    // ========== 8. 初始化超级块 ==========
    let mut filsys = SuperBlock::default();
    filsys.s_isize = DINODEBLK as u16;
    filsys.s_fsize = FILEBLK as u32;
    filsys.s_ninode = ((DINODEBLK * BLOCKSIZ) / DINODESIZ - 4) as u32 ;
    filsys.s_nfree = (FILEBLK - 3) as u32;

    // 初始化空闲 inode 栈
    for i in 0..NICINOD {
        filsys.s_inode[i] = (4 + i) as u32;
    }
    filsys.s_pinode = 0;
    filsys.s_rinode = (NICINOD + 4) as u32;

    // 初始化空闲块成组链接
    let mut block_buf = [0u32; BLOCKSIZ / 4];
    let mut freeblk_id = 511u32;
    // 第一个组长块
    block_buf[0] = (NICFREE - 1) as u32; //上一组空闲块块数
    block_buf[1] = 0; //结束标志
    for i in 2..=NICFREE {
        block_buf[i] = freeblk_id;
        freeblk_id -= 1;
    }
    let page_ptr = self.buffer_pool_manager.fetch_pg((DATASTART/BLOCKSIZ + freeblk_id as usize) as i32).unwrap().unwrap();
    unsafe { ptr::copy_nonoverlapping(block_buf.as_ptr() as *mut u8, page_ptr as *mut u8, BLOCKSIZ) };
    // 后续组长块
    let mut current_blk = freeblk_id + 1;
    while current_blk > 13 {
        block_buf[0] = NICFREE as u32; //上一组空闲块块数
        block_buf[1] = freeblk_id; //上一组组长块
        freeblk_id -= 1;
        for j in 2..=NICFREE {
            block_buf[j] = freeblk_id;
            freeblk_id -= 1;
        }
        let page_ptr = self.buffer_pool_manager.fetch_pg((DATASTART/BLOCKSIZ + freeblk_id as usize) as i32).unwrap().unwrap();
        unsafe { ptr::copy_nonoverlapping(block_buf.as_ptr() as *mut u8, page_ptr as *mut u8, BLOCKSIZ) };
        current_blk = freeblk_id + 1;
    }
    // 剩余空闲块写入超级块
    let mut s_pfreetemp = 0;
    freeblk_id += 1;
    while freeblk_id > 2 {
        filsys.s_free[s_pfreetemp + 1] = freeblk_id;
        s_pfreetemp += 1;
        freeblk_id -= 1;
    }
    filsys.s_free[0] = s_pfreetemp as u32;
    filsys.s_pfree = s_pfreetemp as u16;

    // 写入超级块（块 1）
    let page_ptr = self.buffer_pool_manager.fetch_pg(1).unwrap().unwrap();
    unsafe { ptr::copy_nonoverlapping(&filsys, page_ptr.as_ptr() as *mut SuperBlock, 1) };

    // ========== 9. 刷盘并关闭 ==========
    //unsafe { self.halt() };
    self.buffer_pool_manager.flush_all_pgs(); // 刷盘所有缓冲区
    //fd.flush();
    Ok(())
}
}

use std::fs::File;
use std::path::Path;
use std::fs;
#[test]
fn test() {
    let test_path = "./test_disk.db";
    // 清理旧测试文件
    if Path::new(test_path).exists() {
        let _ = fs::remove_file(test_path);
    }
    // 创建缓冲区池（大小为10，LRU-K的k=5）
    let mut bpm = BufferPoolManager::new(10, 5, test_path).unwrap();

    let mut init_file = std::fs::File::options().write(true).create(true).open(test_path).unwrap();
    init_file.write_all(&[0u8; 500*BLOCKSIZ]);


    drop(init_file); // 释放文件句柄
    let mut s = FileSystem::install(test_path).unwrap();
    println!("fromat");
    s.format(test_path);
    println!("fromat");

    drop(test_path); // 释放文件句柄
    let s = FileSystem::install(test_path);

    let _ = fs::remove_file(test_path);
}