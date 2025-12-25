use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

#[derive(Debug)]
pub struct LRUKReplacer {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    current_timestamp: usize,
    curr_size: usize,
    replacer_size: usize,
    k: usize,

    // history queue
    history_list: Vec<i32>,
    // cache queue (按“第 k 次访问时间戳”从小到大排序；越小越优先淘汰)
    cache_list: Vec<i32>,

    // frame map
    id_frame_map: HashMap<i32, Frame>,
}

#[derive(Debug)]
struct Frame {
    used_cnt: usize,
    evictable: bool,
    // 保存访问时间戳，最多维护最近 k 次（超过 k 时 pop_front）
    timestamp_list: VecDeque<usize>,
}

impl Frame {
    fn new(first_ts: usize) -> Frame {
        let mut q = VecDeque::new();
        q.push_back(first_ts);
        Frame {
            used_cnt: 1,
            evictable: true,
            timestamp_list: q,
        }
    }

    fn increment_used_cnt(&mut self) {
        self.used_cnt += 1;
    }

    fn set_evictable(&mut self, evictable: bool) {
        self.evictable = evictable;
    }

    fn is_evictable(&self) -> bool {
        self.evictable
    }

    fn record_timestamp(&mut self, ts: usize) {
        self.timestamp_list.push_back(ts);
    }

    fn pop_oldest_timestamp(&mut self) {
        self.timestamp_list.pop_front();
    }

    fn kth_timestamp(&self) -> usize {
        // 对于 used_cnt >= k 的 frame，这里一定存在 front
        *self.timestamp_list.front().unwrap()
    }
}

impl LRUKReplacer {
    pub fn new(num_frames: usize, k: usize) -> LRUKReplacer {
        LRUKReplacer {
            inner: Mutex::new(Inner {
                current_timestamp: 0,
                curr_size: 0,
                replacer_size: num_frames,
                k: k,
                history_list: Vec::new(),
                cache_list: Vec::new(),
                id_frame_map: HashMap::new(),
            }),
        }
    }

    // 返回是否成功淘汰，并把被淘汰的 frame_id 写入 frame_id_out
    pub fn evict(&self, frame_id_out: &mut i32) -> bool {
        let mut g = self.inner.lock().unwrap();

        if g.curr_size == 0 {
            return false;
        }

        // 先从 history_list 找
        {
            let mut idx: usize = 0;
            while idx < g.history_list.len() {
                let fid = g.history_list[idx];
                let evictable = match g.id_frame_map.get(&fid) {
                    Some(f) => f.is_evictable(),
                    None => false,
                };
                if evictable {
                    *frame_id_out = fid;
                    g.id_frame_map.remove(&fid);
                    g.history_list.remove(idx);
                    g.curr_size -= 1;
                    return true;
                }
                idx += 1;
            }
        }

        // 再从 cache_list 找
        {
            let mut idx: usize = 0;
            while idx < g.cache_list.len() {
                let fid = g.cache_list[idx];
                let evictable = match g.id_frame_map.get(&fid) {
                    Some(f) => f.is_evictable(),
                    None => false,
                };
                if evictable {
                    *frame_id_out = fid;
                    g.id_frame_map.remove(&fid);
                    g.cache_list.remove(idx);
                    g.curr_size -= 1;
                    return true;
                }
                idx += 1;
            }
        }

        false
    }

    pub fn record_access(&self, frame_id: i32) {
        let mut g = self.inner.lock().unwrap();
    
        if !g.id_frame_map.contains_key(&frame_id) {
            // 新 frame：进入 history
            g.history_list.push(frame_id);
    
            let ts = g.current_timestamp;
            g.id_frame_map.insert(frame_id, Frame::new(ts));
    
            // 新 frame 默认 evictable=true，因此可淘汰数+1（和你之前逻辑一致）
            g.curr_size += 1;
        } else {
            let k = g.k;
            let ts = g.current_timestamp;
    
            // action: 0=不用动队列；1=history->cache；2=cache重排
            let mut action: i32 = 0;
            let mut kts: usize = 0;
    
            {
                // 这一段只操作 frame 本身，绝不碰 g 的其他字段
                let f = g.id_frame_map.get_mut(&frame_id).unwrap();
                f.increment_used_cnt();
    
                if f.used_cnt < k {
                    f.record_timestamp(ts);
                    action = 0;
                } else if f.used_cnt == k {
                    f.record_timestamp(ts);
                    kts = f.kth_timestamp(); // 先把要用的数据拷贝出来
                    action = 1;
                } else {
                    f.pop_oldest_timestamp();
                    f.record_timestamp(ts);
                    kts = f.kth_timestamp(); // 先拷贝出来
                    action = 2;
                }
            } // <- f 在这里结束（drop），释放对 g 的可变借用
    
            // 现在可以安全修改 history_list/cache_list
            if action == 1 {
                remove_from_vec(&mut g.history_list, frame_id);
                insert_cache_sorted_no_conflict(&mut g, frame_id, kts);
            } else if action == 2 {
                remove_from_vec(&mut g.cache_list, frame_id);
                insert_cache_sorted_no_conflict(&mut g, frame_id, kts);
            }
            
        }
    
        g.current_timestamp += 1;
    }
    

    pub fn set_evictable(&self, frame_id: i32, set_evictable: bool) {
        let mut g = self.inner.lock().unwrap();

        let exists = g.id_frame_map.contains_key(&frame_id);
        if !exists {
            return;
        }

        let cur = g.id_frame_map.get(&frame_id).unwrap().is_evictable();
        if cur && !set_evictable {
            g.curr_size -= 1;
        } else if !cur && set_evictable {
            g.curr_size += 1;
        }

        let f = g.id_frame_map.get_mut(&frame_id).unwrap();
        f.set_evictable(set_evictable);
    }

    pub fn remove(&self, frame_id: i32) {
        let mut g = self.inner.lock().unwrap();

        let f_opt = g.id_frame_map.get(&frame_id);
        if f_opt.is_none() {
            return;
        }
        if !f_opt.unwrap().is_evictable() {
            return;
        }

        let used_cnt = f_opt.unwrap().used_cnt;
        if used_cnt >= g.k {
            remove_from_vec(&mut g.cache_list, frame_id);
        } else {
            remove_from_vec(&mut g.history_list, frame_id);
        }

        g.id_frame_map.remove(&frame_id);
        g.curr_size -= 1;
    }

    pub fn size(&self) -> usize {
        let g = self.inner.lock().unwrap();
        g.curr_size
    }
}

fn remove_from_vec(v: &mut Vec<i32>, target: i32) {
    let mut i: usize = 0;
    while i < v.len() {
        if v[i] == target {
            v.remove(i);
            return;
        }
        i += 1;
    }
}

fn insert_cache_sorted_no_conflict(g: &mut Inner, frame_id: i32, kts: usize) {
    // 先算 pos：只需要不可变读 map + 读 cache_list
    let mut pos: usize = 0;
    while pos < g.cache_list.len() {
        let other_id = g.cache_list[pos];
        let other_kts = g.id_frame_map.get(&other_id).unwrap().kth_timestamp();
        if kts <= other_kts {
            break;
        }
        pos += 1;
    }
    // 再真正插入：此时我们只做可变操作
    g.cache_list.insert(pos, frame_id);
}


macro_rules! ensure {
    ($cond:expr) => {
        if !($cond) {
            panic!("ENSURE failed: {}", stringify!($cond));
        }
    };
}

#[test]
fn test() {
    let lru_replacer = LRUKReplacer::new(7, 2);

    lru_replacer.record_access(1);
    lru_replacer.record_access(2);
    lru_replacer.record_access(3);
    lru_replacer.record_access(4);
    lru_replacer.record_access(5);
    lru_replacer.record_access(6);
    lru_replacer.set_evictable(1, true);
    lru_replacer.set_evictable(2, true);
    lru_replacer.set_evictable(3, true);
    lru_replacer.set_evictable(4, true);
    lru_replacer.set_evictable(5, true);
    lru_replacer.set_evictable(6, false);
    ensure!(5 == lru_replacer.size()); // 返回可以淘汰的数量

    // history_list_:[1,2,3,4,5,(6)]. cache_list [1]
    lru_replacer.record_access(1);
    // history_list_:[2,3,4,5,(6)]. cache_list [1]
    let mut value: i32 = -1;
    lru_replacer.evict(&mut value);
    ensure!(2 == value);
    lru_replacer.evict(&mut value);
    ensure!(3 == value);
    lru_replacer.evict(&mut value);
    ensure!(4 == value);
    ensure!(2 == lru_replacer.size());
    // history_list [5,(6)].cache_list [1]

    // Insert new frames 3, 4, and update access history for 5. We should end with [3,1,5,4]
    lru_replacer.record_access(3);
    lru_replacer.record_access(4);
    lru_replacer.record_access(5);
    lru_replacer.record_access(4);
    lru_replacer.set_evictable(3, true);
    lru_replacer.set_evictable(4, true);
    ensure!(4 == lru_replacer.size());
    // history_list_:[(6),3]. cache_list [1,5,4]

    lru_replacer.evict(&mut value);
    ensure!(3 == value);
    ensure!(3 == lru_replacer.size());
    // history_list_:[(6)]. cache_list [1,5,4]

    lru_replacer.set_evictable(6, true);
    ensure!(4 == lru_replacer.size());
    // history_list_:[6]. cache_list [1,5,4]

    lru_replacer.evict(&mut value);
    ensure!(6 == value);
    ensure!(3 == lru_replacer.size());
    // history_list_:[]. cache_list [1,5,4]

    lru_replacer.set_evictable(1, false);
    ensure!(2 == lru_replacer.size());
    ensure!(true == lru_replacer.evict(&mut value));
    ensure!(5 == value);
    ensure!(1 == lru_replacer.size());
    // history_list_:[]. cache_list [(1),4]

    lru_replacer.record_access(1);
    lru_replacer.record_access(1);
    lru_replacer.set_evictable(1, true);
    ensure!(2 == lru_replacer.size());
    ensure!(true == lru_replacer.evict(&mut value));
    ensure!(value == 4);
    // history_list_:[]. cache_list [1]

    ensure!(1 == lru_replacer.size());
    lru_replacer.evict(&mut value);
    ensure!(value == 1);
    ensure!(0 == lru_replacer.size());
    // history_list_:[]. cache_list []

    ensure!(false == lru_replacer.evict(&mut value));
    ensure!(0 == lru_replacer.size());
    lru_replacer.record_access(2);
    lru_replacer.remove(2);
    ensure!(0 == lru_replacer.size());

    println!("ok");
}