use crate::fs::state::*;
use crate::fs::types::*;
use crate::fs::inode::*;
use crate::fs::ballfree::*;
use std::ffi::CStr;
use std::os::raw::c_char;
impl FileSystem {
    pub fn namei(&mut self, _filename: &str) -> u32 {
        // i < dir.size
        for i in 0..self.dir.size {
            // 1. 转换目录项中的d_name为Rust字符串
            let entry_name = unsafe { core::str::from_utf8_unchecked(
                &self.dir.direct[i as usize].d_name
                    // 截断到第一个'\0'
                    .split(|&b| b == 0)
                    .next()
                    .unwrap_or(&[])
            ) };
            // 2.strcmp相等 + d_ino != 0
            if entry_name == _filename && self.dir.direct[i as usize].d_ino != 0 {
                return self.dir.direct[i as usize].d_ino as u32; // 找到返回inode编号
            }
        }
        // 未找到返回NULL（0）
        100
    }

/// 查找当前目录空目录项
/// - name: 要创建的文件名
/// - 返回值: 找到返回目录项位置
pub fn iname(&mut self, name: &str) -> u16 {
    let mut i: usize = 0;
    let mut notfound = 100; 

    // i < DIRNUM && notfound
    while i < DIRNUM && notfound != 0 {
        //目录项 d_ino == 0 表示空项
        if self.dir.direct[i].d_ino == 0 {
            notfound = 0; // 找到空项，置标志为0
            break;        // 退出循环
        }
        i += 1;
    }

    // 未找到空目录项
    if notfound != 0 {
        eprintln!(">The current directory is full!"); 
        return 100; // 
    }

    //找到空目录项，拷贝文件名到 d_name
    let name_bytes = name.as_bytes();
    // 清空原有 d_name
    self.dir.direct[i].d_name.fill(0);
    // 拷贝文件名（截断过长内容）
    let copy_len = name_bytes.len().min(DIRSIZ - 1); // 预留 '\0' 位置
    self.dir.direct[i].d_name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

    // 返回在目录表中的位置
    i as u16
}

}