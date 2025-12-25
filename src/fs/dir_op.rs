use crate::fs::state::*;
use crate::fs::types::*;
use crate::fs::inode::*;
use crate::fs::ballfree::*;
use std::ptr::*;
use std::io::Write;
use std::io::Read;
impl FileSystem {
pub fn _dir(&mut self)  {
    println!("CURRENT DIRECTORY :");

    // 遍历当前目录所有项
    for i in 0..self.dir.size {
        let dir_entry = &self.dir.direct[i as usize];
        if dir_entry.d_ino != DIEMPTY {
            // 1. 解析文件名
            let name = String::from_utf8(
                dir_entry.d_name.iter().take_while(|&&c| c != 0).cloned().collect()
            ).unwrap_or_default();
            print!("{:<20}", name);

            // 2. 获取inode并解析类型/权限
            let temp_inode = self.iget(dir_entry.d_ino as u32); // 获取内存inode
            let mut di_mode = temp_inode.borrow().di_mode;
            let mut x =0;
            if (di_mode & DIDIR) != 0 {
                print!("d");
            } else {
                print!("f");
                x=1;
            }

            // 3. 解析9位权限
            let mut permissions = String::with_capacity(9);
            for j in 0..9 {
                let temp = j % 3;
                let one = di_mode % 2;
                di_mode /= 2;

                permissions.push(if one != 0 {
                    match temp {
                        0 => 'r',
                        1 => 'w',
                        2 => 'x',
                        _ => '!',
                    }
                } else {
                    '-'
                });
            }
            print!("{}\t", permissions);

            // 4. 解析文件大小/块链（目录则输出<dir>）
            if x == 1 {
                // 文件：输出大小+块链
                let size = temp_inode.borrow().di_size;
                print!("file size{}\t", size);
                print!("block chain:");

                let s = (size / BLOCKSIZ as u32) + if size % BLOCKSIZ as u32 != 0 { 1 } else { 0 };
                for j in 0..s as usize {
                    if j < temp_inode.borrow().di_addr.len() {
                        let block = temp_inode.borrow().di_addr[j];
                        print!("{} ", block);
                    }
                }
                println!();

            } else {
                // 目录：输出<dir>
                println!("<dir>");
            }

            // 释放inode
            self.iput(temp_inode);
        }
    }

}

pub fn mkdir(&mut self, user_id_: i32, dirname: &str) {
    // 1. 第一步：检查当前目录是否存在同名项（调用namei）
    let dirid = self.namei(dirname);
    if dirid !=100 { 
        // 存在同名项：获取inode并判断是目录还是文件
        let newinode = self.iget(dirid);
        let inode_ref = newinode.borrow();
        
        if (inode_ref.di_mode & DIDIR) != 0 {
            // 同名目录已存在
            eprintln!("\n{}: directory already existed! !", dirname);
        } else {
            // 与现有文件重名
            eprintln!("\n{}: is a file name, can't create a dir with the same name", dirname);
        }
        self.iput(newinode.clone()); // 释放inode
        return;
    }

    // 2. 第二步：查找当前目录的空项（调用iname）
    let dirpos = self.iname(dirname);
    if dirpos == 100 {
        // 当前目录已满，无法创建新目录
        eprintln!("> [ERROR] dir is full, can't create new directory: {}", dirname);
        return;
    }
    let dirpos = dirpos as usize; // 转换为有效索引

    // 3. 第三步：分配新的inode（调用ialloc）
    let mut newinode = self.ialloc();
    let new_ino = newinode.borrow().i_ino; // 新目录的inode编号

    // 4. 第四步：填写当前目录的空项
    self.dir.direct[dirpos].d_ino = new_ino as u16;
    // 填充目录名到d_name（截断/补空字符）
    let dirname_bytes = dirname.as_bytes();
    let copy_len = dirname_bytes.len().min(DIRSIZ - 1); // 留1字节存空字符
    self.dir.direct[dirpos].d_name[0..copy_len].copy_from_slice(&dirname_bytes[0..copy_len]);
    self.dir.direct[dirpos].d_name[copy_len] = 0; // 空字符结尾
    self.dir.size += 1; // 当前目录项数量+1

    let mut cur_inode_ref = self.cur_path_inode.borrow_mut();
    cur_inode_ref.di_size = self.dir.size as u32 * (DIRSIZ + 2) as u32;
    let cur_ino = cur_inode_ref.i_ino;
    //println!("cur_inode_ref.di_size{}",cur_inode_ref.di_size);
    drop(cur_inode_ref);
    self.iput(self.cur_path_inode.clone());
    self.cur_path_inode = self.iget(cur_ino);

    // 5. 第五步：初始化新目录的数据块（. 和 ..）
    let buf_len = BLOCKSIZ / (DIRSIZ + 2); // 计算目录项数量（DIRSIZ+2=22字节/项）
    let mut buf = vec![DirEntry::default(); buf_len];
    
    // 清空所有目录项
    for x in 0..buf_len {
        buf[x].d_name.fill(0);
        buf[x].d_ino = 0;
    }

    // 填充 . 目录项（指向自身）
    buf[0].d_name[0] = b'.';
    buf[0].d_name[1] = 0;
    buf[0].d_ino = new_ino as u16;

    // 填充 .. 目录项（指向上级目录）
    buf[1].d_name[0] = b'.';
    buf[1].d_name[1] = b'.';
    buf[1].d_name[2] = 0;
    buf[1].d_ino = self.cur_path_inode.borrow().i_ino as u16;

    // 6. 第六步：分配物理块并写入数据（调用balloc + fetch_pg）
    let block = self.balloc(); // 分配新的物理块号
    let block_idx = (DATASTART / BLOCKSIZ + block as usize) as i32;
    let page_opt = self.buffer_pool_manager.fetch_pg(block_idx);
    
    let mut page = page_opt.unwrap().unwrap();

    // 将buf拷贝到页数据
    let buf_ptr = buf.as_ptr() as *const u8;
    // 安全拷贝：Page.data和buf都是固定长度，BLOCKSIZ足够容纳
    page.copy_from_slice(unsafe { std::slice::from_raw_parts(buf_ptr, BLOCKSIZ) });

    // 7. 第七步：填充新inode的属性
    newinode.borrow_mut().di_size = 2 * (DIRSIZ as u32 + 2); // 大小：. + .. 两个目录项
    newinode.borrow_mut().di_number = 1;                     // 关联计数初始化为1
    newinode.borrow_mut().di_addr[0] = block as u16;                // 物理块号赋值

    // 7.1 权限/UID/GID赋值（区分普通用户/超权用户）
    if (user_id_ as usize) < USERNUM {
        // 普通用户：使用user表的默认权限
        let user = &self.user[user_id_ as usize];
        newinode.borrow_mut().di_mode = user.u_default_mode | DIDIR; // 拼接目录类型
        newinode.borrow_mut().di_uid = user.u_uid;
        newinode.borrow_mut().di_gid = user.u_gid;
    } else {
        // 超权用户
        let pwd_idx = user_id_ as usize - USERNUM;
        newinode.borrow_mut().di_mode = USERMODE | DIDIR; // 拼接目录类型
        newinode.borrow_mut().di_uid = self.pwd[pwd_idx].p_uid;
        newinode.borrow_mut().di_gid = self.pwd[pwd_idx].p_gid;
    }

    // 8. 第八步：释放inode（调用iput）
    self.iput(newinode);

}
    /// 目录跳转（Rust原生版，无match，严格复刻原C逻辑）
    /// - force: 1=强制跳转，0=权限校验
    /// - dirname: 目标目录名（Rust字符串）
    /// - 返回值: 1=成功，-1=失败
    pub fn chdir(&mut self, force: i32, dirname: &str) -> i32 {
        // 1. 查找目标目录inode编号
        let dirid = self.namei(dirname);
        if dirid == 100 {
            eprintln!(">{} does not existed", dirname);
            return -1;
        }

        // 2. 获取目标inode（无match，用is_none+unwrap）
        let newinode = self.iget(dirid);

        // 3. 校验：目标是否为文件
        if (newinode.borrow().di_mode & DIFILE) != 0 {
            eprintln!(">cannot use the command on file!");
            self.iput(newinode.clone());
            return -1;
        }

        // 4. 权限校验（force=0时检查执行权限）
        if force == 0 && self.access(self.user_id as i32, newinode.clone(), EXICUTE)==0 {
            eprintln!(">has not access to the directory {}", dirname);
            self.iput(newinode.clone());
            return -1;
        }

        // 5. 计算当前目录数据块相关参数
        let cur_inode_ref = self.cur_path_inode.borrow_mut();
        let size = self.dir.size * (std::mem::size_of::<DirEntry>() as i32);
        let block = (size / BLOCKSIZ as i32) + if size % BLOCKSIZ as i32 != 0 { 1 } else { 0 };
        let pre_block = (cur_inode_ref.di_size / BLOCKSIZ as u32) + 
            if cur_inode_ref.di_size % BLOCKSIZ as u32 != 0 { 1 } else { 0 };
        drop(cur_inode_ref); // 释放可变借用

        // 6. 写入完整数据块
        let mut i = 0;
        while i < (size / BLOCKSIZ as i32) {
            // 分配新块（直到成功）
            if i >= pre_block as i32 {
                let mut new_block = self.balloc();
                while new_block == DISKFULL {
                    new_block = self.balloc();
                }
                self.cur_path_inode.borrow_mut().di_addr[i as usize] = new_block as u16;
            }
            let block_id = self.cur_path_inode.borrow_mut().di_addr[i as usize];

            // 拷贝数据到缓冲区页
            let page = self.buffer_pool_manager.fetch_pg((DATASTART / BLOCKSIZ + block_id as usize) as i32).unwrap().unwrap();

            let dir_data = unsafe {
                std::slice::from_raw_parts(
                    (&self.dir as *const Directory).cast::<u8>().add(i as usize * BLOCKSIZ),
                    BLOCKSIZ
                )
            };
            page[0..BLOCKSIZ].copy_from_slice(dir_data);
            i += 1;
        }

        // 7. 写入剩余数据块
        if size % BLOCKSIZ as i32 != 0 {
            if i >= pre_block as i32 {
                let mut new_block = self.balloc();
                while new_block == DISKFULL {
                    new_block = self.balloc();
                }
                self.cur_path_inode.borrow_mut().di_addr[i as usize] = new_block as u16;
            }
            let block_id = self.cur_path_inode.borrow_mut().di_addr[i as usize];

            let page = self.buffer_pool_manager.fetch_pg((DATASTART / BLOCKSIZ + block_id as usize) as i32).unwrap().unwrap();
            
            let dir_data = unsafe {
                std::slice::from_raw_parts(
                    (&self.dir as *const Directory).cast::<u8>().add(i as usize * BLOCKSIZ),
                    size as usize % BLOCKSIZ
                )
            };
            page[0..size as usize % BLOCKSIZ].copy_from_slice(dir_data);
            i += 1;
        }

        // 8. 释放多余的磁盘块
        while i < pre_block as i32 {
            let bid = self.cur_path_inode.borrow_mut().di_addr[i as usize] as u32;
            self.bfree(bid);
            i += 1;
        }

        // 9. 更新当前inode大小并释放
        let mut cur_inode_ref = self.cur_path_inode.borrow_mut();
        cur_inode_ref.di_size = self.dir.size as u32 * (DIRSIZ + 2) as u32;
        drop(cur_inode_ref);

        // 10. 切换到新目录inode
        self.cur_path_inode = newinode.clone();
        self.dir.size = (self.cur_path_inode.borrow().di_size / (DIRSIZ + 2) as u32) as i32;

        // 11. 读取新目录数据块
        let total_blocks = (newinode.borrow().di_size / BLOCKSIZ as u32) + 
            if newinode.borrow().di_size % BLOCKSIZ as u32 != 0 { 1 } else { 0 };
        let mut i = 0;
        let mut j = 0;
        while i < total_blocks as usize {
            let block_id = newinode.borrow().di_addr[i];
            // 从缓冲区页拷贝数据
            let page = self.buffer_pool_manager.fetch_pg((DATASTART / BLOCKSIZ + block_id as usize) as i32).unwrap().unwrap();
            let dir_ptr = unsafe { (&self.dir.direct[j] as *const DirEntry).cast::<u8>().add(j * std::mem::size_of::<DirEntry>()) };
            unsafe {
                std::ptr::copy_nonoverlapping(page.as_ptr(), dir_ptr as *mut u8, BLOCKSIZ);
            }

            i += 1;
            j += BLOCKSIZ / (DIRSIZ + 2);
        }
        self.iput(newinode.clone());
        // 跳转成功
        1
    }

    pub fn creat(&mut self, user_id: usize, filename: &str, mode: u16) -> i32 {
        // 1. 检查文件名是否已存在（调用namei）
        let di_ino = self.namei(filename);
        if di_ino != 100 { 
            eprintln!(">'{}' has already existed as file or subdirectory", filename);
            return -1; 
        }

        // 2. 校验当前目录的写/执行权限
        let cur_dir_ino = self.namei("."); // 获取当前目录inode编号
        let cur_inode = self.iget(cur_dir_ino);
        
        // 检查写权限和执行权限
        let has_write = self.access(user_id as i32, cur_inode.clone(), WRITE);
        let has_execute = self.access(user_id as i32, cur_inode.clone(), EXICUTE);
        if has_write==0 || has_execute==0 {
            eprintln!(">failed to creat file because of no authority!");
            self.iput(cur_inode);
            return -1;
        }
        self.iput(cur_inode.clone()); // 释放当前目录inode

        // 3. 分配新的inode并初始化属性
        let new_inode = self.ialloc();
        // 4. 预分配目录项位置
        let di_pos = self.iname(filename);
        if di_pos == 100 {
            eprintln!(">Failed to allocate directory position for {}", filename);
            self.iput(new_inode);
            return -1;
        }
        let di_pos = di_pos as usize;
        {
        let mut new_inode_ref = new_inode.borrow_mut();

        // 3.1 填充inode属性
        new_inode_ref.di_mode = mode;
        let user = &self.user[user_id];
        new_inode_ref.di_uid = user.u_uid;
        new_inode_ref.di_gid = user.u_gid;
        new_inode_ref.di_size = 0;
        new_inode_ref.di_number = 1;

        // 3.2 初始化物理块地址数组
        for i in 0..NADDR {
            new_inode_ref.di_addr[i] = 0;
        }

        // 5. 更新目录和当前路径inode
        self.dir.size += 1; // 目录长度+1
        self.dir.direct[di_pos].d_ino = new_inode_ref.i_ino as u16; // 填充inode编号
        
        // 修改当前路径inode的大小
        let mut cur_inode_ref = self.cur_path_inode.borrow_mut();
        cur_inode_ref.di_size += (DIRSIZ + 2) as u32;
        drop(cur_inode_ref);
        }
        // 6. 释放新inode并返回inode编号
        self.iput(new_inode.clone());

        1
    }

    pub fn deletefd(&mut self, user_id: usize, filename: &str) {
        // 1. 查找目标文件/目录的inode编号
        let dinodeid = self.namei(filename);
        if dinodeid == 100 { 
            eprintln!(">deleted file or directory does not exist!");
            return;
        }

        // 2. 获取目标inode
        let inode = self.iget(dinodeid);

        // 3. 权限校验（写权限）
        if self.access(user_id as i32, inode.clone(), WRITE)==0 {
            eprintln!(">failed to remove file because of no authority!");
            return;
        }

        // 4. 交互确认删除（文件/目录区分提示）
        let is_dir = (inode.borrow().di_mode & DIDIR) != 0;
        let prompt = if is_dir {
            format!(">Are you sure to remove the \"{}\" directory?(y/n):", filename)
        } else {
            format!(">Are you sure to remove the \"{}\" file?(y/n):", filename)
        };

        // 4.1 读取用户输入
        print!("{}", prompt);

        loop {
            let _ = std::io::stdout().flush(); // 刷新输出
            let mut flag = ' ';
            let mut input = [0u8; 1];
            if std::io::stdin().read_exact(&mut input).is_err() {
                eprintln!(">Input error");
                return;
            }
            flag = input[0] as char;

            if flag == 'y' || flag == 'Y' {
                break;
            } else if flag == 'n' || flag == 'N' {
                return;
            }
            // 非y/n则继续循环
        }

        // 5. 目录删除前的非空检查
        if is_dir {
            // 5.1 禁止删除根目录（inode=1）
            let root_inode = self.iget(1);
            if  inode.borrow().i_ino==root_inode.borrow().i_ino{
                eprintln!("root dir can't be deleted");
                return;
            }

            // 5.2 跳转到目标目录检查是否非空
            let chdir_res = self.chdir(ROOT as i32, filename);

            // 5.3 目录项数量>2（. 和 ..）表示非空
            if self.dir.size > 2 {
                eprintln!("DIR {} is NOT EMPTY. You can't remove an unEmpty DIR.", filename);
                // 跳转回上级目录
                let _ = self.chdir( ROOT as i32, "..");
                return;
            }

            // 跳转回上级目录
            let _ = self.chdir(ROOT as i32, "..");


        }

        // 6. 从目录项中删除目标（后续项前移）
        let mut target_idx = 0;
        // 6.1 查找目标inode对应的目录项索引
        for i in 0..self.dir.size {
            if self.dir.direct[i as usize].d_ino == dinodeid as u16 {
                target_idx = i;
                break;
            }
        }

        // 6.2 后续目录项前移
        for i in (target_idx + 1)..self.dir.size {
            self.dir.direct[i as usize- 1] = self.dir.direct[i as usize];
        }

        // 6.3 清空最后一项并更新目录大小
        self.dir.direct[self.dir.size as usize - 1].d_ino = 0;
        self.dir.size -= 1;

        // 7. 更新当前路径inode大小
        let mut cur_inode_ref = self.cur_path_inode.borrow_mut();
        cur_inode_ref.di_size -= (DIRSIZ + 2) as u32;
        drop(cur_inode_ref);

        // 8. 减少inode关联计数并释放
        let mut inode_mut_ref = inode.borrow_mut();
        inode_mut_ref.di_number = inode_mut_ref.di_number.saturating_sub(1); // 防止下溢
        drop(inode_mut_ref);
        self.iput(inode.clone());
    }
}