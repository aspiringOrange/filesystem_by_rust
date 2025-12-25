//! 文件系统核心类型定义

/// =======================
/// 常量定义（宏）
/// =======================
use std::array::from_fn; // 用于初始化固定大小数组
pub const BLOCKSIZ: usize = 512;          // 每块大小
pub const SYSOPENFILE: usize = 40;        // 系统打开文件表最大项数
pub const DIRNUM: usize = 128;            // 每个目录所包含的最大目录项数（文件数）
pub const DIRSIZ: usize = 14;             // 每个目录项名字部分所占字节数 / 文件名长度
pub const PWDSIZ: usize = 12;             // 口令字
pub const PWDNUM: usize = 18;             // 最多可设18个口令登录
pub const NOFILE: usize = 20;             // 每个用户最多可打开20个文件
pub const NADDR: usize = 10;              // 每个i节点最多指向10块
pub const NHINO: usize = 128;             // Hash链表数量（必须为2的幂）
pub const USERNUM: usize = 10;             // 最多允许10个用户登录

pub const DINODESIZ: usize = 32;           // 每个磁盘i节点所占字节
pub const DINODEBLK: usize = 32;           // 所有磁盘i节点共占32个物理块
pub const FILEBLK: usize = 512;            // 目录文件物理块数量

pub const NICFREE: usize = 50;             // 超级块中空闲块数组最大块数
pub const NICINOD: usize = 50;             // 超级块中空闲节点最大数量

pub const DINODESTART: usize = 2 * BLOCKSIZ;               // i节点起始地址
pub const DATASTART: usize = (2 + DINODEBLK) * BLOCKSIZ;   // 数据区起始地址

/// =======================
/// 文件类型
/// =======================

pub const DIEMPTY: u16 = 0o00000;   // 类型为空
pub const DIFILE:  u16 = 0o01000;   // 类型为文件
pub const DIDIR:   u16 = 0o02000;   // 类型为目录

/// =======================
/// 用户权限
/// =======================

// user
pub const UDIREAD:    u16 = 0o00001;   // 创建者可读
pub const UDIWRITE:   u16 = 0o00002;   // 创建者可写
pub const UDIEXICUTE: u16 = 0o00004;   // 创建者可运行

// group
pub const GDIREAD:    u16 = 0o00010;   // 同组可读
pub const GDIWRITE:   u16 = 0o00020;   // 同组可写
pub const GDIEXICUTE: u16 = 0o00040;   // 同组可运行

// other
pub const ODIREAD:    u16 = 0o00100;   // 所有人可读
pub const ODIWRITE:   u16 = 0o00200;   // 所有人可写
pub const ODIEXICUTE: u16 = 0o00400;   // 所有人可运行

/// 用户访问权限
pub const READ:    u16 = 0o01;
pub const WRITE:   u16 = 0o02;
pub const EXICUTE: u16 = 0o04;

pub const DEFAULTMODE: u16 = 0o00777;
pub const USERMODE:    u16 = 0o00157;
pub const ROOTMODE:    u16 = 0o00057;

/// =======================
/// 标志位
/// =======================

pub const IUPDATE: u8 = 0o00002;    // i_flag：i节点被修改
pub const SUPDATE: u8 = 0o00001;    // 超级块修改标志

pub const FREAD:   u8 = 0o00001;
pub const FWRITE:  u8 = 0o00002;
pub const FAPPEND: u8 = 0o00004;

/// =======================
/// 其他
/// =======================

pub const DISKFULL: u32 = 65535;    // 磁盘已满
pub const SEEK_SET: u32 = 0;
pub const ROOT: u16 = 1;            // root 用户

/// =======================
/// 内存 i 节点
/// =======================

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Inode {
    pub i_flag: u8,             // 状态标志
    pub i_ino: u32,             // 磁盘索引节点编号
    pub i_count: u32,           // 访问计数

    pub di_number: u16,         // 文件关联计数
    pub di_mode: u16,           // 存取权限及类型
    pub di_uid: u16,            // 用户id
    pub di_gid: u16,            // 组id
    pub di_size: u32,           // 文件大小
    pub di_addr: [u16; NADDR],  // 文件物理块号
}

impl Default for Inode {
    fn default() -> Self {
        Self {
             i_flag: 0,             // 状态标志
             i_ino: 0,             // 磁盘索引节点编号
             i_count: 0,           // 访问计数

             di_number: 0,         // 文件关联计数
             di_mode: 0,           // 存取权限及类型
             di_uid: 0,            // 用户id
             di_gid: 0,            // 组id
             di_size: 0,           // 文件大小
             di_addr: [0; NADDR],  // 文件物理块号
        }
    }
}

/// =======================
/// 磁盘 i 节点
/// =======================

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Dinode {
    pub di_number: u16,
    pub di_mode: u16,
    pub di_uid: u16,
    pub di_gid: u16,
    pub di_size: u32,
    pub di_addr: [u16; NADDR],
}

impl Default for Dinode {
    fn default() -> Self {
        Self {
             di_number: 0,         // 文件关联计数
             di_mode: 0,           // 存取权限及类型
             di_uid: 0,            // 用户id
             di_gid: 0,            // 组id
             di_size: 0,           // 文件大小
             di_addr: [0; NADDR],  // 文件物理块号
        }
    }
}

/// =======================
/// 超级块
/// =======================

#[repr(C)]
pub struct SuperBlock {
    pub s_isize: u16,                   // i节点块数
    pub s_fsize: u32,                   // 数据块总数
    pub s_nfree: u32,                   // 空闲块数
    pub s_pfree: u16,                   // 空闲块指针
    pub s_free: [u32; NICFREE],          // 空闲块栈
    pub s_ninode: u32,                  // 空闲i节点数
    pub s_pinode: u16,                  // 空闲i节点指针
    pub s_inode: [u32; NICINOD],         // 空闲i节点数组
    pub s_rinode: u32,                  // 铭记i节点
    pub s_fmod: u8,                     // 超级块修改标志
}

impl Default for SuperBlock {
    fn default() -> Self {
        Self {
            s_isize: 0,                   // i节点块数
             s_fsize: 0,                   // 数据块总数
             s_nfree: 0,                   // 空闲块数
             s_pfree: 0,                   // 空闲块指针
             s_free: [0; NICFREE],          // 空闲块栈
             s_ninode: 0,                  // 空闲i节点数
             s_pinode: 0,                  // 空闲i节点指针
             s_inode: [0; NICINOD],         // 空闲i节点数组
             s_rinode: 0,                  // 铭记i节点
             s_fmod: 0,                     // 超级块修改标志    
        }
    }
}

/// =======================
/// 用户密码
/// =======================
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Password {
    pub p_uid: u16,                     // 用户id
    pub p_gid: u16,                     // 组id
    pub username: [u8; PWDSIZ],         // 用户名
    pub password: [u8; PWDSIZ],         // 密码
}

impl Default for Password {
    fn default() -> Self {
        Self {
            p_uid: 0,     
            p_gid: 0,     
            username: [0; PWDSIZ],     
            password: [0; PWDSIZ],     
        }
    }
}

/// =======================
/// 目录项
/// =======================

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DirEntry {
    pub d_name: [u8; DIRSIZ],            // 文件名
    pub d_ino: u16,                      // i节点号
}

// 为 DirEntry 实现 Default（关键：给每个字段设默认值）
impl Default for DirEntry {
    fn default() -> Self {
        Self {
            d_ino: 0,               // 默认 inode 编号为 0（无效值）
            d_name: [0; DIRSIZ],    // 目录名默认填充 0 字节（或空格 b' '）
            // 如果你想默认填空格，替换为：d_name: *b"              ", // 14个空格
        }
    }
}

/// =======================
/// 目录
/// =======================

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Directory {
    pub direct: [DirEntry; DIRNUM],      // 目录表
    pub size: i32,                       // 目录项个数
}

// 核心：Directory 的 Default 实现
impl Default for Directory {
    fn default() -> Self {
        Self {
            // 初始化 DIRNUM 个 DirEntry（每个都用 Default 值）
            direct: from_fn(|_| DirEntry::default()),
            // 目录项数量默认设为 0（表示空目录）
            size: 0,
        }
    }
}

/// =======================
/// 系统打开文件表项
/// =======================
use std::cell::RefCell;
use std::rc::Rc;
pub type InodeRef = Rc<RefCell<Inode>>;
#[repr(C)]
pub struct file {
    pub f_flag: u8,                      // 文件操作标志
    pub f_count: u32,                    // 引用计数
    pub f_inode: InodeRef,                    // 指向内存i节点
    pub f_off: u32,                      // 读写指针
}

impl Default for file {
    fn default() -> Self {
        Self {
            f_flag: 0,     
            f_count: 0,     
            f_inode: Rc::new(RefCell::new(Inode::default())),     
            f_off: 0,     
        }
    }
}

/// =======================
/// 用户表
/// =======================

#[repr(C)]
pub struct User {
    pub u_default_mode: u16,             // 用户类别
    pub u_uid: u16,                      // 用户ID
    pub u_gid: u16,                      // 用户组ID
    pub u_ofile: [u16; NOFILE],          // 用户打开文件表
}

impl Default for User {
    fn default() -> Self {
        Self {
            u_default_mode: 0,     
            u_uid: 0,     
            u_gid: 0,     
            u_ofile: [SYSOPENFILE as u16 + 1;NOFILE]    
        }
    }
}