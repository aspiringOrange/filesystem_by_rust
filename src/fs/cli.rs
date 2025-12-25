use crate::fs::state::*;
use crate::fs::types::*;
use std::env::consts;
use std::io::Read;
use std::io::Write;
use std::ffi::CString;
use crate::fs::fmt::*;
use crate::fs::dir_op::*;
use std::process::Command;
use std::ptr;

fn copy_str_to_bytes_2(s: &str, dest: &mut [u8]) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(dest.len());
    dest[..len].copy_from_slice(&bytes[..len]);
    // 剩余位置填 0
    for i in len..dest.len() {
        dest[i] = 0;
    }
}

impl FileSystem {
/// 改变窗口输出路径函数
/// - precord: 路径输出缓冲区（可变字符串，替代C的char*）
/// - cname: 要改变的目录名（&str，替代C的char*）
/// - last_inode_id: 当前路径inode号
pub fn chpath(&mut self,precord: &mut String, cname: &str, last_inode_id: i32) {
    // 匹配目标目录名
    match cname {
        // 跳转到当前目录，直接返回
        "." => return,

        // 跳转到上级目录
        ".." => {
            // 找到字符串末尾位置（替代C的while循环找'\0'）
            let mut i = precord.len();
            // 根目录（inode=1且路径是root），禁止回退
            if precord.ends_with("root") && last_inode_id == 1 {
                return;
            }
            // 从后往前找最后一个'/'，截断路径
            for j in (0..i).rev() {
                if precord.as_bytes()[j] == b'/' {
                    // 截断路径到最后一个'/'位置（替代C的precord[j] = '\0'）
                    precord.truncate(j);
                    return;
                }
            }
        },

        // 跳转到子目录，拼接路径
        _ => {
            // 拼接 "/" + 目录名（替代C的strcat）
            precord.push('/');
            precord.push_str(cname);
        }
    }
}


fn getch(&self) -> u8 {
    let mut input = [0u8; 1];
    loop {
        if std::io::stdin().read_exact(&mut input).is_ok() {
            if(input[0]!=b'\r'&&input[0]!=b'\n'){
                break;
            }
        }

    }
    input[0]
}


/// 核心CLI交互函数（复刻原 C 版逻辑）
pub fn cli(disk_path: &str) {

    // 初始化缓冲区
    let mut read_buffer = [0u8; 10 * BLOCKSIZ]; // 文件读取缓冲区
    let mut cbuf = [0u8; 5120];                 // 临时拷贝缓冲区
    let mut temp_file_size = 0;                // 临时文件大小
    let mut cname = [0u8; 14];                 // 临时文件名
    let mut login_name = [0u8; PWDSIZ];        // 登录名缓冲区
    let mut login_password = [0u8; PWDSIZ];    // 密码缓冲区
    let mut cache = String::new(); // 初始化空字符串
    let mut input_str = String::new(); // 初始化空字符串
    
    let mut exit_flag = 1;                     // 退出标志
    let mut tprecord = String::new();          // 提示符字符串
    let mut cmd = [0u8; 10];                   // 命令缓冲区
    let mut s = FileSystem::install(disk_path).unwrap();
    //let mut s = FileSystem::new_empty(disk_path).unwrap();
    s.user_id = -1;                         // 初始未登录

    // 3. 打印欢迎信息并询问格式化
    println!("Virtual Ubuntu File System.");
    println!("\nDo you want to format the disk? (y(es)/n(o)) :");
    
    // 读取用户格式化选择（模拟getch）
    let format_choice = s.getch();
    io::stdin().read_line(&mut cache).is_ok();
    if format_choice == b'y' {
        println!("\nFormat will erase all context on the disk. Are You Sure? (y(es)/n(o)) :");
        let confirm_choice = s.getch();
        io::stdin().read_line(&mut cache).is_ok();
        if confirm_choice == b'y' {
            // 输入root密码
            print!("Please input rooter password: ");
            let _ = std::io::stdout().flush();

            
            // 核心：读取一行输入到input_str（自动包含换行符，需手动去除）
            io::stdin().read_line(&mut input_str).is_ok();
            // 去除末尾的换行/回车符（\n 或 \r\n）
            let trimmed_str = input_str.trim_end_matches(&['\n', '\r'][..]);
   
            // 验证root密码
            let root_pwd = "root";
            if trimmed_str == root_pwd {
                s.format(disk_path); // 执行格式化
                println!("\n>System is formative now");
            } else {
                println!("\n>Incorrect root password system is not formative");
            }
        }
    }
    println!("\n>install");
    // 4. 装载文件系统
    let mut filesystem = FileSystem::install(disk_path).unwrap();
    filesystem.user_id = -1;                         // 初始未登录
    loop{
    // 5. 登录循环（直到登录成功）
        loop {
            if filesystem.user_id != -1 {
                break; // 登录成功则退出循环
            }

            // 初始化登录缓冲区
            login_name.fill(0);
            login_password.fill(0);

            // 输入用户名
            println!("\nPlease enter your account to log in ");
            print!("\tUSERNAME:   ");
            let _ = std::io::stdout().flush();
            let mut username_input = String::new();
            if std::io::stdin().read_line(&mut username_input).is_ok() {
                // 去除换行符并拷贝到缓冲区
                let username = username_input.trim().as_bytes();
                let copy_len = username.len().min(PWDSIZ - 1);
                login_name[0..copy_len].copy_from_slice(&username[0..copy_len]);
            }

            // 输入密码
            print!("\tPASSWORD:   ");
            let _ = std::io::stdout().flush();
            let mut userpwd_input = String::new();
            if std::io::stdin().read_line(&mut userpwd_input).is_ok() {
                // 去除换行符并拷贝到缓冲区
                let userpwd = userpwd_input.trim().as_bytes();
                let copy_len = userpwd.len().min(PWDSIZ - 1);
                login_password[0..copy_len].copy_from_slice(&userpwd[0..copy_len]);
            }
            println!();

            // 执行登录
            filesystem.user_id = unsafe { filesystem.login(login_name.as_ptr()as *const i8, login_password.as_ptr()as *const i8) };

            // 登录成功处理
            if filesystem.user_id != -1 {
                // 检查是否为root用户
                let is_root = filesystem.user[filesystem.user_id as usize].u_uid == ROOT;
                if !is_root {
                    // 跳转到用户目录
                    let mut name1 = username_input.trim_end_matches(&['\n', '\r'][..]);
                    filesystem.chdir(ROOT as i32, name1);
                    // 构建提示符
                    tprecord = format!("{}@VirtualUbuntu:~/root/{}", name1, name1);
                } else {
                    // root用户提示符
                    tprecord = "root@VirtualUbuntu:~/root".to_string();
                }
                println!("\nLogin success!");
            } else {
                println!("\nLogin failed! Please try again.");
            }
        }
        print!("\n{} ", tprecord);
        io::stdout().flush().unwrap();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).is_ok();
        let mut cmd1 = input.trim_end_matches(&['\n', '\r'][..]);
        while(cmd1.len()==0){
            std::io::stdin().read_line(&mut input).is_ok();
            cmd1 = input.trim_end_matches(&['\n', '\r'][..]);
        }
        let parts: Vec<&str> = cmd1.split_whitespace().collect();
        let cmd = parts[0];
		if (cmd=="ls"){
			filesystem._dir();
        }else if (cmd == "mkdir")
		{
			filesystem.mkdir(filesystem.user_id,parts[1]);
        }else if (cmd == "cd")
		{
			let flag = filesystem.chdir(0,parts[1]);
            if(flag==1){
                let ino = filesystem.cur_path_inode.borrow().i_ino;
                filesystem.chpath(&mut tprecord, parts[1], ino as i32);
            }else{
                println!(">Failed to change directory.Please retry\n");
            }
		}else if (cmd == "creat"){
            filesystem.creat(filesystem.user_id as usize, parts[1], ROOTMODE | GDIREAD | DIFILE);
            let pos_in_sysofile = filesystem.aopen(filesystem.user_id as i32, parts[1], FWRITE as u16);
            let ino: u32 =filesystem.sys_ofile[pos_in_sysofile as usize].f_inode.borrow().i_ino;
            let temp_inode = filesystem.iget(ino);

			if (temp_inode.borrow().i_ino != 0 && temp_inode.borrow().di_size != 0){
                println!(">file open success!");
            }
			filesystem.iput(temp_inode);

            let tfd = filesystem.xfa(parts[1]);
            if(tfd!=-1){
                let sys_otpos = filesystem.user[filesystem.user_id as usize].u_ofile[tfd as usize];					//获取当前文件在系统文件打开表中的位置
				let temp_file_size = filesystem.sys_ofile[sys_otpos as usize].f_inode.borrow().di_size;	//获取当前文件大小
                println!(">Write file [{}],please input:", parts[1]);
                let mut input2 = String::new();
                std::io::stdin().read_line(&mut input2).is_ok();
                unsafe { filesystem.write(tfd, input2.as_ptr() as *const u8, input2.len() as u32) };
            }else{
                println!(">Cannot write a file that is not open or not exixted in current directory");
            }
            unsafe { filesystem.close(filesystem.user_id as u32, tfd as i16) };

        }else if (cmd == "close"){
            let tfd = filesystem.xfa(parts[1]);
            if(tfd!=-1){
                unsafe { filesystem.close(filesystem.user_id as u32, tfd as i16) };
                println!(">File close successful");
            }else{
                println!(">Cannot close a file that is not open or not exixted in current directory");
            }

        }else if (cmd == "delete"){
            unsafe { filesystem.deletefd(filesystem.user_id as usize, parts[1]) };
        
        }else if (cmd == "halt"){
            filesystem.chdir(0,"..");
            // let ino = filesystem.cur_path_inode.clone();
			// filesystem.iput(ino);	//***
			unsafe { filesystem.halt() };
			break;
        
        }
        else if (cmd == "logout"){
            filesystem.chdir(0,"..");
            // let ino = filesystem.cur_path_inode.clone();
			// filesystem.iput(ino);	//***
			unsafe { filesystem.logout(filesystem.user_id as u16) };
            filesystem.user_id = -1;
        }
        else if (cmd == "aopen"){
            if(parts[2]=="-r"){
                filesystem.aopen(filesystem.user_id, parts[1], FREAD as u16);
            }
            if(parts[2]=="-w"){
                filesystem.aopen(filesystem.user_id, parts[1], FWRITE as u16);
            }
            if(parts[2]=="-a"){
                filesystem.aopen(filesystem.user_id, parts[1], FAPPEND as u16);
            }

        }
        else if (cmd == "help"){
            println!(" $ ls\t\t显示当前目录\n $ mkdir\t创建目录\n $ cd\t跳转目录\n $ creat\t创建文件\n $ close\t关闭已经打开的文件\n $ delete\t删除文件\n $ cruser\t创建用户\n $ halt\t\t关机");
            println!(" $ logout\t退出账户\n $ aopen\t打开文件\n $ copy\t复制文件\n $ pst\t粘贴文件\n $ help\t\t显示帮助\n $ fmt\t\t格式化\n $ rd\t\t读文件\n $ wr\t\t写文件\n $ cls\t\t清屏");

        }
        else if (cmd == "fmt"){
            if (filesystem.user[filesystem.user_id as usize].u_uid != ROOT)
                {//当前用户不是rooter
                    println!(">Failed to format the disk because of unqualified authority!");
                }
            else{
                filesystem.format(disk_path);				//格式化
				filesystem = FileSystem::install(disk_path).unwrap();				//装载
                filesystem.user_id = -1;
            }
            }
        else if (cmd == "rd"){
            let tfd = filesystem.xfa(parts[1]);
            if (tfd != -1)
			{//文件存在且被打开
				let sys_otpos = filesystem.user[filesystem.user_id as usize].u_ofile[tfd as usize];					//获取当前文件在系统文件打开表中的位置
				let temp_file_size=filesystem.sys_ofile[sys_otpos as usize].f_inode.borrow().di_size;	//获取当前文件大小
                let mut x=0;
				unsafe { x = filesystem.read(tfd, read_buffer.as_ptr() as *mut u8) };						//将文件内容读入缓冲区
                let ascii_str: String = read_buffer
                .iter()
                .take(x as usize) 
                .map(|&b| if b!=0 { b as char } else { ' ' }) // 非打印字符用 '.' 替代
                .collect();

                println!("\n>Content of the file: {}",ascii_str);
			}else{
                println!(">Cannot read a file that is not open or not exixted in current directory");
            }
    
        }
        else if (cmd == "wr"){
            let tfd = filesystem.xfa(parts[1]);
            if (tfd != -1)
			{//文件存在且被打开
				let sys_otpos = filesystem.user[filesystem.user_id as usize].u_ofile[tfd as usize];					//获取当前文件在系统文件打开表中的位置
				let temp_file_size=filesystem.sys_ofile[sys_otpos as usize].f_inode.borrow().di_size;	//获取当前文件大小

                println!(">Write file [{}],please input:", parts[1]);			//输出提示信息
                let mut input2 = String::new();
                std::io::stdin().read_line(&mut input2).is_ok();
                unsafe { filesystem.write(tfd, input2.as_ptr() as *const u8, input2.len() as u32) };
            }else{
                println!(">Cannot write a file that is not open or not exixted in current directory");
            }
    
        }
        else if (cmd == "cls"){
            Command::new("cmd").arg("/c").arg("cls").status();
    
        }
        else if (cmd == "copy"){
            filesystem.aopen(filesystem.user_id, parts[1], FREAD as u16);
            let tfd = filesystem.xfa(parts[1]);
            if (tfd != -1)
			{//文件存在且被打开
				let sys_otpos = filesystem.user[filesystem.user_id as usize].u_ofile[tfd as usize];					//获取当前文件在系统文件打开表中的位置
                filesystem.cpy(parts[1],&mut cbuf,&mut temp_file_size);
                
            }else{
                println!(">Cannot copy a file that is not open or not exixted in current directory");
            }
    
        }
        else if (cmd == "pst"){
            filesystem.creat(filesystem.user_id as usize, parts[1], ROOTMODE | GDIREAD | DIFILE);
            let pos_in_sysofile = filesystem.aopen(filesystem.user_id, parts[1], WRITE as u16);
            let ino: u32 =filesystem.sys_ofile[pos_in_sysofile as usize].f_inode.borrow().i_ino;
            let temp_inode = filesystem.iget(ino);

			if (temp_inode.borrow().i_ino != 0 && temp_inode.borrow().di_size != 0){
                println!(">file open success!");
            }
			filesystem.iput(temp_inode);
            let tfd = filesystem.xfa(parts[1]);
            if (tfd != -1)
			{//文件存在且被打开
				let sys_otpos = filesystem.user[filesystem.user_id as usize].u_ofile[tfd as usize];					//获取当前文件在系统文件打开表中的位置
                filesystem.pst(parts[1],&mut cbuf,temp_file_size);
                
            }else{
                println!(">Cannot pst a file that is not open or not exixted in current directory");
            }

        }
        else if (cmd == "cruser"){
            if(filesystem.cur_path_inode.borrow().i_ino!=1){
				println!("\nplease creat in root dir");
				continue;
			}
            println!("please scanf user name:");
            let mut uname1 = String::new();
            std::io::stdin().read_line(&mut uname1).is_ok();
            let uname=uname1.trim_end_matches(&['\n', '\r'][..]);
			println!("please scanf user upword:");
			let mut upwd1 = String::new();
            std::io::stdin().read_line(&mut upwd1).is_ok();
            let upwd=upwd1.trim_end_matches(&['\n', '\r'][..]);
            {
				let i: usize = 0;
                for i in 0..PWDNUM-1{
                    if (filesystem.pwd[i].username[0] == b' ') {
                        copy_str_to_bytes_2(&uname, &mut filesystem.pwd[i].username);
                        copy_str_to_bytes_2(&upwd, &mut filesystem.pwd[i].password);
						filesystem.pwd[i].p_uid = i as u16 +1;
						filesystem.pwd[i].p_gid = i as u16 +1;
						break;//用户名匹配退出循环
					}
                }

				if (i == PWDNUM  as usize)
				{//用户名不匹配
					println!(">Too many users\n");
					continue;
				}

				let pageptr = filesystem.buffer_pool_manager.fetch_pg(DATASTART as i32 /BLOCKSIZ as i32 +2).unwrap().unwrap();
                unsafe { ptr::copy_nonoverlapping(filesystem.pwd.as_ptr() as *mut u8, pageptr as *mut u8, BLOCKSIZ) };
				filesystem.mkdir(i as i32 +1+USERNUM as i32, &uname);
				//用户名匹配
			}


        }
        else{
            println!(">No such a command! Please retry");
        }


    }
}
}

use std::fs::File;
use std::io::{self, Seek, SeekFrom};
use std::path::Path;
use std::fs;
#[test]
fn test() {
    let test_path = "./test_disk.db";
    //清理旧测试文件
    // if Path::new(test_path).exists() {
    //     let _ = fs::remove_file(test_path);
    // }


    // let mut init_file = std::fs::File::options().write(true).create(true).open(test_path).unwrap();
    // init_file.write_all(&[0u8; (DINODEBLK + FILEBLK + 2)*BLOCKSIZ]);


    //drop(init_file); // 释放文件句柄
    FileSystem::cli(test_path);
    //let _ = fs::remove_file(test_path);

}