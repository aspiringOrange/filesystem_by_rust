use std::collections::LinkedList;
use std::hash::{Hash, Hasher};

/// 可拓展哈希表的 Bucket 实现
#[derive(Debug, Clone)]  // 实现 Clone trait
struct Bucket<K, V> {
    size: usize,
    depth: i32,
    list: LinkedList<(K, V)>,
}

impl<K: Eq + Clone, V: Clone> Bucket<K, V> {
    /// 创建新的 Bucket
    fn new(size: usize, depth: i32) -> Self {
        Bucket {
            size,
            depth,
            list: LinkedList::new(),
        }
    }

    /// 检查 Bucket 是否已满
    fn is_full(&self) -> bool {
        self.list.len() == self.size
    }

    /// 获取 Bucket 的局部深度
    fn get_depth(&self) -> i32 {
        self.depth
    }

    /// 增加 Bucket 的局部深度
    fn increment_depth(&mut self) {
        self.depth += 1;
    }

    /// 获取 Bucket 中的所有元素（返回可变引用）
    fn get_items(&mut self) -> &mut LinkedList<(K, V)> {
        &mut self.list
    }

    /// 查找指定 key 对应的 value
    fn find(&self, key: &K, value: &mut Option<V>) -> bool {
        for (k, v) in &self.list {
            if k == key {
                *value = Some(v.clone());
                return true;
            }
        }
        false
    }

    /// 移除指定 key 的元素
    fn remove(&mut self, key: &K) -> bool {
        let mut new_list = LinkedList::new();
        let mut found = false;

        // 遍历原链表，筛选保留非目标元素
        while let Some(item) = self.list.pop_front() {
            if &item.0 == key {
                found = true;
            } else {
                new_list.push_back(item);
            }
        }

        self.list = new_list;
        found
    }

    /// 插入或更新 key-value 对
    fn insert(&mut self, key: K, value: V) -> bool {
        // 先检查是否是更新操作（无论桶是否满）
        for (k, v) in &mut self.list {
            if k == &key {
                *v = value;
                return self.list.len() <= self.size; // 桶不满返回true，满返回false
            }
        }

        // 桶已满，无法插入新元素
        if self.is_full() {
            return false;
        }

        // 插入新元素
        self.list.push_back((key, value));
        true
    }
}

/// 可拓展哈希表实现（单线程版本，无锁）
#[derive(Debug)]
pub struct ExtendibleHashTable<K, V> {
    global_depth: i32,          // 直接存储，无锁
    bucket_size: usize,
    num_buckets: i32,           // 直接存储，无锁
    dir: Vec<Box<Bucket<K, V>>>, // 直接存储，无锁
}

impl<K: Eq + Hash + Clone, V: Clone> ExtendibleHashTable<K, V> {
    /// 创建新的可拓展哈希表
    pub fn new(bucket_size: usize) -> Self {
        let mut dir = Vec::new();
        let init_bucket = Box::new(Bucket::new(bucket_size, 0));
        dir.push(init_bucket);

        ExtendibleHashTable {
            global_depth: 0,
            bucket_size,
            num_buckets: 1,
            dir,
        }
    }

    /// 获取全局深度
    pub fn get_global_depth(&self) -> i32 {
        self.global_depth
    }

    /// 获取指定目录索引对应的 Bucket 局部深度
    pub fn get_local_depth(&self, dir_index: usize) -> i32 {
        if dir_index < self.dir.len() {
            self.dir[dir_index].get_depth()
        } else {
            -1
        }
    }

    /// 获取 Bucket 数量
    pub fn get_num_buckets(&self) -> i32 {
        self.num_buckets
    }

    /// 查找指定 key 对应的 value
    pub fn find(&self, key: K) -> Option<V> {
        let index = self.index_of(&key);
        if index < self.dir.len() {
            let mut value = None;
            if self.dir[index].find(&key, &mut value) {
                return value;
            }
        }
        None
    }

    /// 插入 key-value 对
    pub fn insert(&mut self, key: K, value: V) {
        self.insert_one(key, value);
    }

    /// 移除指定 key 的元素
    pub fn remove(&mut self, key: K) -> bool {
        let index = self.index_of(&key);
        if index < self.dir.len() {
            return self.dir[index].remove(&key);
        }
        false
    }

    // ===== 内部方法 =====
    /// 计算 key 对应的目录索引
    fn index_of(&self, key: &K) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        let hash_val = hasher.finish();
        
        let mask = (1 << self.global_depth) - 1;
        (hash_val & mask as u64) as usize
    }
/// 内部：插入单个 key-value 对（
fn insert_one(&mut self, key: K, value: V) {
    // 1. 计算目录索引，获取当前桶

    let dir_index = self.index_of(&key);
    // 检查索引有效性
    if dir_index >= self.dir.len() {
        return;
    }

    // 尝试插入当前桶
    let inserted = {
        let bucket = &mut self.dir[dir_index];
        bucket.insert(key.clone(), value.clone())
    };

    // 插入成功直接返回
    if inserted {
        return;
    }
    
    // 2. 获取全局深度/局部深度
    let global_depth = self.global_depth;
    let local_depth = self.dir[dir_index].get_depth();

    // 保存原桶的指针地址
    let indexed_bucket_ptr = &*self.dir[dir_index] as *const Bucket<K, V>;

    // 3. 局部深度 == 全局深度：扩展目录 + 分裂桶
    if local_depth == global_depth {
        // 增加全局深度
        self.global_depth += 1;
        
        // 扩展目录长度（翻倍）
        let old_dir_len = self.dir.len();
        self.dir.resize(old_dir_len * 2, Box::new(Bucket::new(self.bucket_size, 0)));

        // 创建两个新桶（深度+1）
        let new_depth = local_depth + 1;
        let first_bucket = Box::new(Bucket::new(self.bucket_size, new_depth));
        let second_bucket = Box::new(Bucket::new(self.bucket_size, new_depth));

        let dir_extend_size = 1 << (self.global_depth - 1);
        // 重新哈希原桶元素
        // 先克隆原桶元素
        let current_bucket = unsafe { &*indexed_bucket_ptr }; 
        let mut items = current_bucket.list.clone();
        // 直接赋值目标索引的目录项
        self.dir[dir_index] = first_bucket;
        self.dir[dir_index + dir_extend_size] = second_bucket.clone();

        // 复制原有目录指针
        for i in 0..dir_extend_size {
            if self.dir[i + dir_extend_size].get_depth() == 0 {
                self.dir[i + dir_extend_size] = self.dir[i].clone();
            }
        }

        for (k, v) in items {
            let index = self.index_of(&k);
            if index < self.dir.len() {
                self.dir[index].insert(k, v);
            }
        }
    }

    // 4. 局部深度 < 全局深度：仅分裂桶
    if local_depth < global_depth {
        // 创建两个新桶（深度+1）
        let new_depth = local_depth + 1;
        let first_bucket = Box::new(Bucket::new(self.bucket_size, new_depth));
        let second_bucket = Box::new(Bucket::new(self.bucket_size, new_depth));

        // 重新哈希原桶元素
        let current_bucket = unsafe { &*indexed_bucket_ptr }; // 安全：原桶未被释放
        let mut items = current_bucket.list.clone();

        // 遍历所有目录项，替换指向原桶的指针
        let dir_size = self.dir.len();
        for i in 0..dir_size {
            let current_bucket_ptr = &*self.dir[i] as *const Bucket<K, V>;
            if current_bucket_ptr == indexed_bucket_ptr {
                let top_bit = (i >> local_depth) & 1;
                if top_bit == 0 {
                    self.dir[i] = first_bucket.clone(); 
                } else {
                    self.dir[i] = second_bucket.clone(); 
                }
            }
        }

        for (k, v) in items {
            let index = self.index_of(&k);
            if index < self.dir.len() {
                self.dir[index].insert(k, v);
            }
        }
    }

    // 增加桶计数
    self.num_buckets += 1;

    // 递归插入当前元素
    self.insert_one(key, value);
}
    
}

// ===== 测试用例 =====
#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn extended_test() {
        let mut table = ExtendibleHashTable::new(2);
        
        // 插入一系列元素
        table.insert(1, "a");
        table.insert(2, "b");
        table.insert(3, "c");
        assert_eq!(table.get_global_depth(), 1);
        table.insert(4, "d");
        assert_eq!(table.get_global_depth(), 2);
        table.insert(5, "e");
        table.insert(6, "f");
        table.insert(7, "g");
        assert_eq!(table.get_global_depth(), 2);
        table.insert(8, "h");
        assert_eq!(table.get_global_depth(), 3);
        table.insert(9, "i");

        // 验证查找功能
        assert_eq!(table.find(9), Some("i"));
        assert_eq!(table.find(8), Some("h"));
        assert_eq!(table.find(7), Some("g"));
        assert_eq!(table.find(6), Some("f"));
        assert_eq!(table.find(5), Some("e"));
        assert_eq!(table.find(4), Some("d"));
        assert_eq!(table.find(3), Some("c"));
        assert_eq!(table.find(2), Some("b"));
        assert_eq!(table.find(1), Some("a"));
        assert_eq!(table.find(10), None);
        
        // 验证局部深度
        assert_eq!(table.get_global_depth(), 4);
        assert_eq!(table.get_local_depth(0), 2);
        assert_eq!(table.get_local_depth(1), 1);
        assert_eq!(table.get_local_depth(2), 2);
        assert_eq!(table.get_local_depth(7), 4);
        
        
        // 验证删除功能
        assert!(table.remove(8));
        assert!(table.remove(4));
        assert!(table.remove(1));
        assert!(!table.remove(20));
        
        // 打印结果（对应 printf("ok\n")）
        println!("ok");
    }
}