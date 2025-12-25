use std::cell::RefCell;
use std::rc::Rc;
use std::ptr;
use crate::fs::types::*;
use crate::fs::bufferpool::*;
use crate::fs::inode::*;
pub struct FileSystem {
    pub buffer_pool_manager: BufferPoolManager,
    pub hinode: Vec<Vec<InodeRef>>, // NHINO 个桶，每桶一个 Vec
    pub dir: Directory,
    pub sys_ofile: [file; SYSOPENFILE],
    pub filsys: SuperBlock,
    pub pwd: [Password; PWDNUM],
    pub user: [User; USERNUM],
    pub disk: std::fs::File,          // 原 fd
    pub cur_path_inode: InodeRef,          // 建议存 inode id，不直接存裸指针
    pub user_id: i32,
    pub cpcache: Vec<u8>,                // 复制缓冲区
}

impl FileSystem {
    /// 初始化空的文件系统实例
    pub fn new_empty(disk_path: &str) -> Result<Self, std::io::Error> {

        let disk = std::fs::File::options()
            .read(true)
            .write(true)
            .create(true)
            .open(disk_path)?;

        let buffer_pool_manager = BufferPoolManager::new(1000,5, disk_path).unwrap();

        // 2. 初始化所有字段（用 Default 或默认值）
        Ok(Self {
            buffer_pool_manager: buffer_pool_manager,
            hinode: vec![Vec::new(); NHINO], // inode哈希桶初始化
            dir: Directory::default(),
            sys_ofile: std::array::from_fn(|_| file::default()), // 数组初始化
            filsys: SuperBlock::default(),
            pwd: std::array::from_fn(|_| Password::default()),
            user: std::array::from_fn(|_| User::default()),
            disk,
            cur_path_inode: Rc::new(RefCell::new(Inode::default())), // 初始值，后续用 iget 覆盖
            user_id: 0,
            cpcache: Vec::with_capacity(BLOCKSIZ),
        })
    }


    /// 当无法打开磁盘文件时返回错误
    pub fn install(disk_path: &str) -> Result<Self, std::io::Error> {

        let mut fs = FileSystem::new_empty(disk_path)?;

        // 2. 装载密码表
        let pwd_block:i32 = (DATASTART as i32) / (BLOCKSIZ as i32) + 2;

        let page_pwd = fs.buffer_pool_manager.fetch_pg(pwd_block);
        // 内存拷贝
        unsafe {
            // pageptr.as_ptr()：缓冲区起始地址（转为Password类型的常量指针）
            // pwd.as_mut_ptr()：密码表的可变指针
            ptr::copy_nonoverlapping(
                page_pwd.unwrap().unwrap().as_ptr() as *const Password,
                fs.pwd.as_mut_ptr(),
                PWDNUM,
            );
        }

        // 打印密码表
        for i in 0..4 {
            let raw_username = String::from_utf8_lossy(&fs.pwd[i].username); // 存临时值
            let username = raw_username.trim_end_matches('\0'); // 借用这个长生命周期的值
            let raw_password = String::from_utf8_lossy(&fs.pwd[i].password); // 存临时值
            let password = raw_password.trim_end_matches('\0'); // 借用这个长生命周期的值
            //println!("username:{}   password:{}", username, password);
        }

        // 3. 读取超级块
        let sb_page = fs.buffer_pool_manager.fetch_pg(1); // 超级块在第1块
        unsafe {
            ptr::copy_nonoverlapping(
                sb_page.unwrap().unwrap().as_ptr() as *const SuperBlock,
                &mut fs.filsys as *mut SuperBlock,
                1
            );
        }


        // 7. 读取根目录inode
        fs.cur_path_inode = fs.iget(1);
        //println!("{}",fs.cur_path_inode.borrow_mut().di_size);
        // 8. 初始化当前目录
        fs.dir = Directory {
            size: fs.cur_path_inode.borrow().di_size as i32 / (DIRSIZ as i32 + 2),
            direct: [DirEntry { d_ino: 0, d_name: [0; DIRSIZ] }; DIRNUM],
        };
        //println!("{}",fs.cur_path_inode.borrow().di_size);
        // 初始化目录项为空格+0号inode
        for i in 0..DIRNUM {
            fs.dir.direct[i].d_name = *b"              "; // 14个空格
            fs.dir.direct[i].d_ino = 0;
        }

        // 9. 读取磁盘目录项到目录表
        let entries_per_block = BLOCKSIZ / (DIRSIZ + 2);
        let x = fs.dir.size / entries_per_block as i32;
        let mut i = 0;

        let y = BLOCKSIZ as i32 / (DIRSIZ as i32 + 2);

        // 处理完整磁盘块
        if x >= y {
            for i in (0..x).step_by(y as usize) {
                let block_num = DATASTART / BLOCKSIZ + fs.cur_path_inode.borrow().di_addr[i as usize] as usize;
                let pageptr = fs.buffer_pool_manager.fetch_pg(block_num as i32);
                let dest_ptr = &mut fs.dir.direct[entries_per_block * (i as usize)] as *mut DirEntry;
                unsafe {
                    ptr::copy_nonoverlapping(
                        pageptr.unwrap().unwrap().as_ptr() as *const DirEntry,
                        dest_ptr,
                        entries_per_block
                    );
                }
            }
        }

        // 处理剩余不足一个块的目录项
        let remaining_block = DATASTART / BLOCKSIZ + fs.cur_path_inode.borrow().di_addr[i] as usize;
        let pageptr = fs.buffer_pool_manager.fetch_pg(remaining_block as i32);
        let remaining_size = fs.cur_path_inode.borrow().di_size as usize % BLOCKSIZ;
        let dest_ptr = &mut fs.dir.direct[entries_per_block * (i as usize)] as *mut DirEntry;
        unsafe {
            ptr::copy_nonoverlapping(
                pageptr.unwrap().unwrap().as_ptr() as *const DirEntry,
                dest_ptr,
                remaining_size
            );
        }

        // 10. 刷新所有缓冲区
        fs.iput(fs.cur_path_inode.clone());
        //fs.buffer_pool_manager.flush_all_pgs()?;
        Ok(fs)
    }
    
}

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
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
    let s = FileSystem::install(test_path);

    let _ = fs::remove_file(test_path);

}