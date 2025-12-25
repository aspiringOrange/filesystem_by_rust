use std::collections::{HashMap, LinkedList};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use crate::fs::lruk::*;
use crate::fs::extend_hashtable::*;
use crate::fs::types::*;

// 常量定义（对应C中的BLOCKSIZ）
const INVALID_PAGE_ID: i32 = -1;
const INVALID_FRAME_ID: i32 = -1;

// 类型别名
type PageId = i32;
type FrameId = i32;
type Page = [u8; BLOCKSIZ];

// 缓冲区池管理器
#[derive(Debug)]
pub struct BufferPoolManager {
    pool_size: usize,
    bucket_size: usize,
    pages: Vec<Page>,                                    // 缓冲区页面数组
    page_table: ExtendibleHashTable<PageId, FrameId>,    // 明确指定泛型参数
    frame2pageid: HashMap<FrameId, PageId>,              // 帧到页面的映射
    replacer: LRUKReplacer,                              // 修正类型名
    free_list: LinkedList<FrameId>,                      // 空闲帧列表
    disk_file: File,                                     // 磁盘文件句柄
}

impl BufferPoolManager {
    // 创建新的缓冲区池管理器
    pub fn new(pool_size: usize, replacer_k: usize, disk_path: &str) -> io::Result<Self> {
        // 打开磁盘文件（读写模式）
        let disk_file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .open(disk_path)?;

        // 初始化页面数组
        let mut pages = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            pages.push([0u8; BLOCKSIZ]);
        }

        // 初始化空闲列表
        let mut free_list = LinkedList::new();
        for i in 0..pool_size {
            free_list.push_back(i as FrameId);
        }

        Ok(BufferPoolManager {
            pool_size,
            bucket_size: 4,
            pages,
            page_table: ExtendibleHashTable::new(4),
            frame2pageid: HashMap::new(),
            replacer: LRUKReplacer::new(pool_size, replacer_k),
            free_list,
            disk_file,
        })
    }

    // 获取缓冲区池大小
    pub fn get_pool_size(&self) -> usize {
        self.pool_size
    }

    // 获取页面数据指针（返回不可变引用）
    pub fn get_pages(&self) -> &[Page] {
        &self.pages
    }

    // 获取可变页面数据指针
    fn get_pages_mut(&mut self) -> &mut [Page] {
        &mut self.pages
    }

    // 从磁盘读取页面到指定帧
    fn read_page_from_disk(
        disk_file: &mut File,
        page_id: PageId,
        frame_id: FrameId,
        pages: &mut [Page], 
    ) -> io::Result<()> {
        let offset = (page_id as u64) * (BLOCKSIZ as u64);
        disk_file.seek(SeekFrom::Start(offset))?;
        let page_buf = &mut pages[frame_id as usize];
        disk_file.read_exact(page_buf)?;
        Ok(())
    }

    // 将帧中的页面写入磁盘
    fn write_page_to_disk(
        disk_file: &mut File, 
        page_id: PageId,
        frame_id: FrameId,
        pages: &[Page],    
    ) -> io::Result<()> {
        //println!("page_id{}",page_id);
        let offset = (page_id as u64) * (BLOCKSIZ as u64);
        disk_file.seek(SeekFrom::Start(offset))?;
        let page_buf = &pages[frame_id as usize];
        disk_file.write_all(page_buf)?;
        disk_file.sync_all()?;
        Ok(())
    }

    // 获取指定页面
    pub fn fetch_pg(&mut self, page_id: PageId) -> io::Result<Option<&mut Page>> {
        // 1. 检查页面是否已在缓冲区中
        let mut frame_id = INVALID_FRAME_ID;
        if let Some(fid) = self.page_table.find(page_id) {
            //println!("fetch{}",page_id);
            frame_id = fid;
            let frame_idx = frame_id as usize;
            self.replacer.record_access(frame_id);
            self.replacer.set_evictable(frame_id, false);
            return Ok(Some(&mut self.get_pages_mut()[frame_idx]));
        }

        // 2. 检查空闲列表
        if let Some(fid) = self.free_list.pop_front() {
            //println!("read{}",page_id);
            // 从磁盘读取页面
            Self::read_page_from_disk(
                &mut self.disk_file,
                page_id,
                fid,
                &mut self.pages,  
            )?;
            
            // 更新映射关系
            self.frame2pageid.insert(fid, page_id);
            self.page_table.insert(page_id, fid);
            
            // 更新替换器
            self.replacer.record_access(fid);
            self.replacer.set_evictable(fid, true);
            
            let frame_idx = fid as usize;
            return Ok(Some(&mut self.get_pages_mut()[frame_idx]));
        }

        // 3. 没有空闲帧，尝试驱逐
        if self.replacer.size() > 0 {
            //println!("read{}",page_id);
            let mut evict_frame = INVALID_FRAME_ID;
            if !self.replacer.evict(&mut evict_frame) {
                return Ok(None);
            }

            // 获取被驱逐页面的ID
            let evict_page_id = *self.frame2pageid.get(&evict_frame).unwrap();
           // println!("evict_page_id{}",evict_page_id);
            // 将被驱逐页面写回磁盘
            Self::write_page_to_disk(
                &mut self.disk_file,
                evict_page_id,
                evict_frame,
                &self.pages,
            )?;
            
            // 移除旧映射
            self.page_table.remove(evict_page_id);
            self.frame2pageid.remove(&evict_frame);
            
            // 从磁盘读取新页面
            Self::read_page_from_disk(
                &mut self.disk_file,
                page_id,
                evict_frame,
                &mut self.pages,  
            )?;
            
            // 更新映射关系
            self.frame2pageid.insert(evict_frame, page_id);
            self.page_table.insert(page_id, evict_frame);
            
            // 更新替换器
            self.replacer.record_access(evict_frame);
            self.replacer.set_evictable(evict_frame, true);
            
            let frame_idx = evict_frame as usize;
            return Ok(Some(&mut self.get_pages_mut()[frame_idx]));
        }

        // 无可用帧
        Ok(None)
    }

    // 刷新指定页面到磁盘
    pub fn flush_pg(&mut self, page_id: PageId) -> io::Result<bool> {
        let mut frame_id = INVALID_FRAME_ID;
        if let Some(fid) = self.page_table.find(page_id) {
            frame_id = fid;
            Self::write_page_to_disk(
                &mut self.disk_file,
                page_id,
                frame_id,
                &self.pages,
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    // 刷新所有页面到磁盘
    pub fn flush_all_pgs(&mut self) -> io::Result<()> {
        // 1. 先复制数据到局部变量，释放对 self.frame2pageid 的不可变借用
        let frame_page_pairs: Vec<(FrameId, PageId)> = self.frame2pageid
            .iter()
            .map(|(&fid, &pid)| (fid, pid))
            .collect();
        
        // 2. 遍历局部变量（无 self 借用），此时可安全可变借用 self
        for (frame_id, page_id) in frame_page_pairs {
            //println!("page_id {}",page_id);
            Self::write_page_to_disk(&mut self.disk_file, page_id, frame_id, &self.pages)?;
        }
        Ok(())
    }

    // 删除指定页面
    pub fn delete_pg(&mut self, page_id: PageId) -> io::Result<bool> {
        let mut frame_id = INVALID_FRAME_ID;
        if let Some(fid) = self.page_table.find(page_id) {
            frame_id = fid;
            // 写回磁盘
            Self::write_page_to_disk(
                &mut self.disk_file,
                page_id,
                frame_id,
                &self.pages,
            )?;
            // 清空页面数据
            let frame_idx = frame_id as usize;
            self.get_pages_mut()[frame_idx].fill(0);
            // 更新元数据
            self.page_table.remove(page_id);
            self.frame2pageid.remove(&frame_id);
            self.replacer.remove(frame_id);
            self.free_list.push_back(frame_id);
            return Ok(true);
        }
        Ok(true)
    }
}

// 实现Drop trait确保资源正确释放
// impl Drop for BufferPoolManager {
//     fn drop(&mut self) {
//         // 退出前刷新所有页面
//         //let _ = self.flush_all_pgs();
//     }
// }

// 使用示例
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_buffer_pool_basic() {
        let test_path = "./test_disk.db";
        // 清理旧测试文件
        if Path::new(test_path).exists() {
            let _ = fs::remove_file(test_path);
        }

        // 创建缓冲区池（大小为10，LRU-K的k=5）
        let mut bpm = BufferPoolManager::new(10, 5, test_path).unwrap();

        let mut init_file = File::options().write(true).create(true).open(test_path).unwrap();
        init_file.write_all(&[0u8; 2*BLOCKSIZ]);
        drop(init_file); // 释放文件句柄

        // 获取新页面
        let page1 = bpm.fetch_pg(1).unwrap().unwrap();
        // 修改页面数据
        page1[0] = 0xAA;
        page1[1] = 0xBB;

        // 刷新页面到磁盘
        assert!(bpm.flush_pg(1).unwrap());

        // 删除页面
        assert!(bpm.delete_pg(1).unwrap());

        // 验证文件存在
        assert!(Path::new(test_path).exists());

         // 4. 重新获取页面（从磁盘读取），验证数据是否正确
         let page1_reload = bpm.fetch_pg(1).unwrap().unwrap();
         assert_eq!(page1_reload[0], 0xAA, "第一个字节应为0xAA");
         assert_eq!(page1_reload[1], 0xBB, "第二个字节应为0xBB");
         println!("重新读取页面验证成功：数据与写入一致");

         // 5. 直接从磁盘文件读取验证（绕开缓冲区，验证落盘是否成功）
        let mut disk_file = File::open(test_path).unwrap();
        let mut buffer = [0u8; BLOCKSIZ];
        // 定位到page_id=1的偏移位置
        disk_file.seek(SeekFrom::Start(BLOCKSIZ as u64 * 1)).unwrap();
        disk_file.read_exact(&mut buffer).unwrap();
        assert_eq!(buffer[0], 0xAA, "磁盘文件中第一个字节应为0xAA");
        assert_eq!(buffer[1], 0xBB, "磁盘文件中第二个字节应为0xBB");
        println!("直接读取磁盘文件验证成功：数据已持久化");

        // 清理测试文件
        let _ = fs::remove_file(test_path);
    }
}