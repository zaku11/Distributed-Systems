mod domain;

pub use crate::broadcast_public::*;
pub use crate::executors_public::*;
pub use crate::stable_storage_public::*;
pub use crate::system_setup_public::*;
pub use domain::*;

pub mod broadcast_public {
    use crate::executors_public::ModuleRef;
    use crate::{PlainSenderMessage, StableStorage, StubbornBroadcastModule, SystemAcknowledgmentMessage, SystemBroadcastMessage, SystemMessageContent, SystemMessageHeader, SystemMessage};
    use std::collections::{HashSet, HashMap};
    use uuid::Uuid;

    pub trait PlainSender: Send + Sync {
        fn send_to(&self, uuid: &Uuid, msg: PlainSenderMessage);
    }

    pub trait ReliableBroadcast: Send {
        fn broadcast(&mut self, msg: SystemMessageContent);

        fn deliver_message(&mut self, msg: SystemBroadcastMessage);

        fn receive_acknowledgment(&mut self, msg: SystemAcknowledgmentMessage);
    }

    // In ReliableStorage 
    // We will be keeping : 
    // pending, actual_msges and delivered

    const TRANSFORM_BYTES_SIZE : usize = 16;

    fn serialize_hdr(header : SystemMessageHeader) -> Vec<u8> {
        let mut vec = Vec::new();
        vec.extend(header.message_source_id.as_bytes());
        vec.extend(header.message_id.as_bytes());
        vec
    }
    fn hdr_to_string(header : SystemMessageHeader) -> String {
        let mut ans = String::new();
        ans += &(header.message_source_id.to_string());
        ans += &(header.message_id.to_string());
        ans
    }
    fn deserialize_hdr(bytes : Vec<u8>) -> SystemMessageHeader {
        let mut fst = [0; TRANSFORM_BYTES_SIZE];
        let mut snd = [0; TRANSFORM_BYTES_SIZE];
        for i in 0..TRANSFORM_BYTES_SIZE {
            fst[i] = bytes[i];
        }
        for i in 0..TRANSFORM_BYTES_SIZE {
            snd[i] = bytes[i + TRANSFORM_BYTES_SIZE];
        }
        SystemMessageHeader {
            message_source_id : Uuid::from_bytes(fst),
            message_id : Uuid::from_bytes(snd),
        }
    }
    fn serialize_hdr_with_uuid(uuid : Uuid, hdr : SystemMessageHeader) -> Vec <u8> {
        let mut vec = Vec::new(); 
        vec.extend(uuid.as_bytes());
        vec.extend(serialize_hdr(hdr));
        vec
    }
    fn deserialize_hdr_with_uuid(mut bytes : Vec<u8> ) -> (Uuid, SystemMessageHeader) {
        let mut fst = [0; TRANSFORM_BYTES_SIZE];
        for i in 0..TRANSFORM_BYTES_SIZE {
            fst[i] = bytes[i];
        }
        let other_part = bytes.split_off(TRANSFORM_BYTES_SIZE);
        (Uuid::from_bytes(fst), deserialize_hdr(other_part))
    }
    fn serialize_msg(msg : SystemMessage) -> Vec<u8> {
        let mut vec = Vec::new();
        vec.extend(serialize_hdr(msg.header));
        vec.extend(msg.data.msg);
        vec
    }
    fn deserialize_msg(mut bytes : Vec<u8>) -> SystemMessage {
        let hdr = deserialize_hdr(bytes.clone());
        let other_part = bytes.split_off(2 * TRANSFORM_BYTES_SIZE); 
        let content = SystemMessageContent{
            msg : other_part,
        };
        SystemMessage {
            header : hdr,
            data : content,
        }
    }

    struct MyReliableBroadcast {
        stubborn : ModuleRef<StubbornBroadcastModule>,
        storage : Box<dyn StableStorage>,
        how_many_proc : usize, 
        callback : Box<dyn Fn(SystemMessage) + Send>,
        ack : HashMap<SystemMessageHeader, HashSet<Uuid>>,
        id : Uuid,
        current_msg_number : u128,
        pending : HashSet<(Uuid, SystemMessageHeader)>,
        delivered : HashSet<(Uuid , SystemMessageHeader)>,
        actual_msges : HashMap<SystemMessageHeader, SystemMessage>,
    }

    const MAX_FILE_CONTENT : usize = 65536;


    impl MyReliableBroadcast {

        fn store_schema(&mut self, mut data : Vec<u8>, name : String) {
            let mut iter = 0;
            while data.len() > MAX_FILE_CONTENT {
                let last_part = data.split_off(MAX_FILE_CONTENT);
                let new_name = name.clone() + &iter.to_string();
                self.storage.put(&new_name, &data).unwrap();
                iter += 1;
                data = last_part;
            }
            let new_name = name + &iter.to_string();
            self.storage.put(&new_name, &data).unwrap();
        }
        fn retrieve_schema(&mut self, name : String) -> Vec<u8> {
            let mut iter = 0;
            let mut content = Vec::new();
            loop {
                let new_name = name.clone() + &iter.to_string();
                match self.storage.get(&new_name) {
                    Some(some_bytes) => {
                        content.extend(some_bytes);
                    },
                    None => {
                        break;
                    },
                }
                iter += 1;
            }
            content
        }

        fn retrieve_pending(&mut self) {
            let mut pending_bytes = self.retrieve_schema("keys_pending".to_string());
            let mut new_pending : HashSet<(Uuid, SystemMessageHeader)> = HashSet::new();
            while !pending_bytes.clone().is_empty() {
                if pending_bytes.len() < 3 * TRANSFORM_BYTES_SIZE {
                    log::warn!("Invalid number of bytes when trying to retrieve pending")
                }
                else {
                    let (uuid, hdr) = deserialize_hdr_with_uuid(pending_bytes.clone());
                    pending_bytes = pending_bytes.split_off(3 * TRANSFORM_BYTES_SIZE); 
                    new_pending.insert((uuid, hdr));
                }
            }
            self.pending = new_pending 
        }
        fn store_pending(&mut self) {
            let mut keys_pending = Vec::new();
            for (uuid, hdr) in self.pending.clone() {
                keys_pending.extend(serialize_hdr_with_uuid(uuid, hdr));
            }
            self.store_schema(keys_pending, "keys_pending".to_string());
        }
        fn retrieve_delivered(&mut self) {
            let mut delivered_bytes = self.retrieve_schema("keys_delivered".to_string());
            let mut new_delivered : HashSet<(Uuid, SystemMessageHeader)> = HashSet::new();

            while !delivered_bytes.clone().is_empty() {
                if delivered_bytes.len() < 3 * TRANSFORM_BYTES_SIZE {
                    log::warn!("Invalid number of bytes when trying to retrieve delivered")
                }
                else {
                    let (uuid, hdr) = deserialize_hdr_with_uuid(delivered_bytes.clone());
                    delivered_bytes = delivered_bytes.split_off(3 * TRANSFORM_BYTES_SIZE); 
                    new_delivered.insert((uuid, hdr));
                }
            }
            self.delivered = new_delivered
        }
        fn store_delivered(&mut self) {
            let mut keys_delivered = Vec::new();
            for (id, hdr) in self.delivered.clone() {
                keys_delivered.extend(serialize_hdr_with_uuid(id, hdr));
            }
            self.store_schema(keys_delivered, "keys_delivered".to_string());
        }
        fn retrieve_actual_msges(&mut self) {
            let mut actual_msges_bytes = self.retrieve_schema("keys_actual_msges".to_string());
            let mut new_actual_msges : HashMap<SystemMessageHeader, SystemMessage> = HashMap::new();

            while !actual_msges_bytes.clone().is_empty() {
                if actual_msges_bytes.len() < 2 * TRANSFORM_BYTES_SIZE {
                    log::warn!("Invalid number of bytes when trying to retrieve actual msges")
                }
                else {
                    let hdr = deserialize_hdr(actual_msges_bytes.clone());
                    actual_msges_bytes = actual_msges_bytes.split_off(2 * TRANSFORM_BYTES_SIZE);
                    new_actual_msges.insert(
                        hdr, deserialize_msg(self.storage.get(&hdr_to_string(hdr)).unwrap())
                    );
                }
            }
            self.actual_msges = new_actual_msges
        }
        fn store_actual_msges(&mut self) {
            let mut keys_msges = Vec::new();
            for (hdr, msg) in self.actual_msges.clone() {
                keys_msges.extend(serialize_hdr(hdr));
                self.storage.put(&hdr_to_string(hdr), &serialize_msg(msg)).unwrap();
            }
            self.store_schema(keys_msges, "keys_actual_msges".to_string());
        }
        fn retrieve_current_msg_number(&mut self) {
            match self.storage.get("current_msg_number") {
                Some(msg_bytes) => {
                    if msg_bytes.len() != 16 {
                        log::warn!("Invalid number of bytes when trying to retrieve current message number")
                    }
                    let mut arr : [u8; 16] = [0; 16];
                    for i in 0..15 {
                        arr[i] = msg_bytes[i];
                    }
                    self.current_msg_number = u128::from_be_bytes(arr); 
                }
                None => {}
            }

        }
        fn store_current_msg_number(&mut self) {
            self.storage.put("current_msg_number", &self.current_msg_number.to_be_bytes()).unwrap();
        }
    }

    impl ReliableBroadcast for MyReliableBroadcast {
        fn broadcast(&mut self, msg: SystemMessageContent) {
            let system_msg = SystemMessage {
                header : SystemMessageHeader {
                    message_source_id : self.id,
                    message_id : Uuid::from_u128(self.current_msg_number),
                },
                data : SystemMessageContent {
                    msg : msg.msg, // Will this work with instances on different machines?
                },
            };
            self.pending.insert((self.id, system_msg.header));
            self.store_pending();

            self.stubborn.send(SystemBroadcastMessage {
                forwarder_id : self.id,
                message : system_msg.clone(),
            });
            self.actual_msges.insert(system_msg.header, system_msg);
            self.store_actual_msges();

            self.current_msg_number += 1;
            self.store_current_msg_number();
        }

        fn deliver_message(&mut self, msg: SystemBroadcastMessage) {
            let source_id = msg.message.header.message_source_id;
            let forwarder_id = msg.forwarder_id;
            let entry = (source_id, msg.message.header);

            if !self.pending.contains(&entry) {
                self.pending.insert(entry);
                self.stubborn.send(SystemBroadcastMessage {
                    forwarder_id : self.id,
                    message : msg.message.clone(),
                }); // Unleash the storm!
                self.actual_msges.insert(msg.message.header.clone(), msg.message.clone());

                if !self.ack.contains_key(&msg.message.header) {
                    self.ack.insert(msg.message.header, HashSet::new());
                }
                let ack_set = self.ack.get_mut(&msg.message.header).unwrap(); 
                if !ack_set.contains(&forwarder_id) {
                    ack_set.insert(forwarder_id);

                    let hdr = msg.message.header;
                    let new_delivered_entry = (source_id, hdr);
                    if !self.delivered.contains(&new_delivered_entry) && ack_set.len() > self.how_many_proc / 2 {
                        let full_msg = msg.message;
                        (*self.callback)(full_msg);
                        self.delivered.insert(new_delivered_entry);
                        self.actual_msges.remove(&hdr);
                        self.store_delivered();
                    }
                }
            }

        }
        fn receive_acknowledgment(&mut self, msg: SystemAcknowledgmentMessage) {
            self.stubborn.send((msg.proc, msg.hdr));
        }        
    }

    pub fn build_reliable_broadcast(
        sbeb: ModuleRef<StubbornBroadcastModule>,
        storage: Box<dyn StableStorage>,
        id: Uuid,
        processes_number: usize,
        delivered_callback: Box<dyn Fn(SystemMessage) + Send>,
    ) -> Box<dyn ReliableBroadcast> {

        let mut new_broadcast = Box::new(MyReliableBroadcast {
            stubborn : sbeb.clone(),
            storage : storage,
            id : id,
            how_many_proc : processes_number,
            callback : delivered_callback,
            delivered : HashSet::new(),
            pending : HashSet::new(),
            ack : HashMap::new(),
            actual_msges : HashMap::new(),
            current_msg_number : 1,
        });
        new_broadcast.retrieve_delivered();
        new_broadcast.retrieve_pending();
        new_broadcast.retrieve_actual_msges();
        new_broadcast.retrieve_current_msg_number();
        for (_, val) in &new_broadcast.pending {
            sbeb.send(SystemBroadcastMessage {
                forwarder_id : id,
                message : new_broadcast.actual_msges.get(val).unwrap().clone(),
            });

        }

        new_broadcast
    }

    pub trait StubbornBroadcast: Send {
        fn broadcast(&mut self, msg: SystemBroadcastMessage);

        fn receive_acknowledgment(&mut self, proc: Uuid, msg: SystemMessageHeader);

        fn send_acknowledgment(&mut self, proc: Uuid, msg: SystemAcknowledgmentMessage);

        fn tick(&mut self);
    }

    struct MyStubbornBroadcast {
        all_others : HashSet<Uuid>,
        link : Box <dyn PlainSender>,
        still_left : HashSet<(Uuid, Uuid)>, // first param is message number, the second is proccess
        messages : HashMap<Uuid, SystemBroadcastMessage>,
    }

    impl StubbornBroadcast for MyStubbornBroadcast {
        fn broadcast(&mut self, msg: SystemBroadcastMessage) {
            self.messages.insert(msg.message.header.message_id, msg.clone());
            for other in &self.all_others {
                self.still_left.insert((msg.clone().message.header.message_id, *other));
                self.link.send_to(other, PlainSenderMessage::Broadcast(msg.clone()));
            }
        }
        fn receive_acknowledgment(&mut self, proc: Uuid, msg: SystemMessageHeader) {
            self.still_left.remove(&(msg.message_id, proc));
        }
        fn send_acknowledgment(&mut self, proc: Uuid, msg: SystemAcknowledgmentMessage) {
            self.link.send_to(&proc, PlainSenderMessage::Acknowledge(msg));
        }
        fn tick(&mut self) {
            for (msg_num, proc) in &self.still_left {
                self.link.send_to(proc, PlainSenderMessage::Broadcast(self.messages[msg_num].clone()));
            }
        }
    }

    pub fn build_stubborn_broadcast(
        link: Box<dyn PlainSender>,
        processes: HashSet<Uuid>,
    ) -> Box<dyn StubbornBroadcast> {
        Box::new(MyStubbornBroadcast {
            all_others : processes,
            link : link,
            still_left : HashSet::new(),
            messages : HashMap::new(),
        })
    }
}

pub mod stable_storage_public {
    use std::path::PathBuf;
    use std::fs::*;
    use std::io::Write;
    use std::str;
    use std::fs::File;
    use std::io::Read;

    const MAX_FILE_NAME : usize = 255;
    const MAX_FILE_CONTENT : usize = 65536;
    
    pub trait StableStorage: Send {
        fn put(&mut self, key: &str, value: &[u8]) -> Result<(), String>;

        fn get(&self, key: &str) -> Option<Vec<u8>>;
    }
    struct MyStableStorage {
        dir : PathBuf,
    }
    fn key_to_path(key : &str) -> PathBuf {
        let v = key.split('/');
        let mut ans = PathBuf::new();
        for part in v {
            ans.push(part);
        }
        ans
    }
    fn transform_key(key : &str) -> String {
        let mut new_key = key.to_owned();
        new_key = "!".to_owned() + &new_key + &"!".to_owned();
        new_key.insert(new_key.len() / 2, '/');
        new_key
    }
    
    impl StableStorage for MyStableStorage {

        fn put(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
            if key.len() > MAX_FILE_NAME || value.len() > MAX_FILE_CONTENT {
                return Err("Too long name or too long content.".to_string());
            } 
            let mut path_to_tmp = self.dir.clone();
            let my_key = transform_key(key); 

            path_to_tmp.push("not_yet_inserted");
            path_to_tmp.push(key_to_path(&my_key));
            create_dir_all(path_to_tmp.clone()).unwrap();

            path_to_tmp.push("file");
            if path_to_tmp.is_file() {
                std::fs::remove_file(path_to_tmp.clone()).unwrap();
            }
            let mut file = std::fs::OpenOptions::new().write(true).create(true).open(path_to_tmp.clone()).unwrap();

            file.write_all(value).unwrap();

            let mut path_to_normal = self.dir.clone();
            path_to_normal.push(key_to_path(&my_key));

            create_dir_all(path_to_normal.clone()).unwrap();
            
            path_to_normal.push("file");

            rename(path_to_tmp, path_to_normal).unwrap();

            Ok(())
        }
        fn get(&self, key: &str) -> Option<Vec<u8>> {
            if key.len() > MAX_FILE_NAME {
                return None;
            }
            let my_key = transform_key(key);

            let mut file_path = self.dir.clone();
            file_path.push(key_to_path(&my_key));
            file_path.push("file");

            if !file_path.is_file() {
                return None;
            }
            
            let mut content = Vec::new();
            let mut file = File::open(file_path).unwrap();
            file.read_to_end(&mut content).unwrap();

            Some(content)
        }
    }

    pub fn build_stable_storage(root_storage_dir: PathBuf) -> Box<dyn StableStorage> {
        let mut tmp_path = root_storage_dir.clone();
        tmp_path.push("not_yet_inserted"); // This will be necessary so our put is atomic
        create_dir_all(tmp_path).unwrap();
        Box::new(MyStableStorage {
            dir : root_storage_dir,
        })
    }

}

pub mod executors_public {
    use std::fmt;
    use std::time::Duration;
    use std::time::SystemTime;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::Condvar;
    use std::ops::DerefMut;
    use std::collections::BinaryHeap;
    use core::cmp::Ordering;
    use std::thread::JoinHandle;
    use crossbeam_channel::{unbounded, Receiver, Sender};
    use std::collections::HashMap;
    use std::sync::Weak;




    pub trait Message: fmt::Debug + Clone + Send + 'static {}
    impl<T: fmt::Debug + Clone + Send + 'static> Message for T {}

    pub trait Handler<M: Message>
    where
        M: Message,
    {
        fn handle(&mut self, msg: M);
    }

    #[derive(Debug, Clone)]
    pub struct Tick {}


    struct TickEntry {
        start_time : SystemTime,
        dur : Duration, 
        action_number : usize,
    }

    impl Ord for TickEntry{
        fn cmp(&self, other: &Self) -> Ordering {
            // self.start_time.cmp(&other.start_time)
            other.start_time.cmp(&self.start_time) // Hopefully changing the order here is enough
        }
    }
    impl PartialOrd for TickEntry{
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }
    impl PartialEq for TickEntry{
        fn eq(&self, other: &Self) -> bool {
            self.start_time == other.start_time
        }
    }
    impl Eq for TickEntry {}
    
    pub type ClosureMsg = Box<dyn Fn() -> () + Send>;

    pub enum WorkerMsg {
        NewModule((u128, Arc<Mutex<dyn Send + 'static>>)),
        ExecuteMsg(ClosureMsg),
        RemModule(u128),
        EndingMsg,
    }
    
    pub struct System {
        msges_go_here : Sender<WorkerMsg>, // This will be create ONCE

        executor_thread : Option<JoinHandle<()>>,
        tick_thread : Option<JoinHandle<()>>,
        last_number : u128,

        last_action_number : usize,
        tick_actions : Arc<Mutex<Vec<ClosureMsg>>>,
        
        tick_request_data : Arc<(Mutex<BinaryHeap<TickEntry>>, Condvar)>,   
        are_we_being_dropped : Arc<Mutex<bool>>,
    }


    impl System {
        pub fn request_tick<T: Handler<Tick> + Send>(&mut self, requester: &ModuleRef<T>, delay: Duration) {
            
            let (tick_queue, sleeper) = &*self.tick_request_data;
            let req_clone = requester.clone();

            let mut tick_queue_locked = tick_queue.lock().unwrap(); 

            self.tick_actions.lock().unwrap().push(Box::new(
                move || {
                    req_clone.force_send(Tick{});
                }
            ));

            tick_queue_locked.push(TickEntry{
                start_time : SystemTime::now() + delay,
                dur : delay,
                action_number : self.last_action_number
            });
            
            self.last_action_number += 1;

            if tick_queue_locked.peek().unwrap().action_number == self.last_action_number - 1 {
                sleeper.notify_one();
            }
        }

        pub fn register_module<T : Send + 'static> (&mut self, module: T) -> ModuleRef<T> {
            let module_wrapped = Arc::new(Mutex::new(module));

            let module_ref = ModuleRef {
                number : self.last_number,
                module : Arc::downgrade(&module_wrapped),
                output_channel : self.msges_go_here.clone(),
                how_many_instances : Arc::new(Mutex::new(1)),
            };

            self.msges_go_here.send(WorkerMsg::NewModule((self.last_number, module_wrapped))).unwrap();
            self.last_number += 1;

            module_ref
        }

        pub fn new() -> Self {
            let (msges_go_here, msg_queue) : (Sender<WorkerMsg>, Receiver<WorkerMsg>) = unbounded();
            let tick_request_data : Arc<(Mutex<BinaryHeap<TickEntry>>, Condvar)> = Arc::new((Mutex::new(BinaryHeap::new()), Condvar::new()));   

            let tick_actions : Arc<Mutex<Vec<ClosureMsg>>> = Arc::new(Mutex::new(Vec::new()));
            let are_we_being_dropped = Arc::new(Mutex::new(false));

            let executor_thread = std::thread::spawn(move || { 
                let mut module_table : HashMap<u128, Arc<Mutex<dyn Send + 'static>>> = HashMap::new();
                while let Ok(msg) = msg_queue.recv() {
                    match msg {
                        WorkerMsg::NewModule((id, module_ref)) => {
                            module_table.insert(id, module_ref);
                        },
                        WorkerMsg::ExecuteMsg(closure) => {
                            closure();
                        },
                        WorkerMsg::RemModule(id) => {
                            module_table.remove(&id);
                        },
                        WorkerMsg::EndingMsg => {
                            break;
                        },
                    }
                }
            });
            let req_clone = tick_request_data.clone();
            let action_table = tick_actions.clone();
            let are_we_being_dropped_clone = are_we_being_dropped.clone();
                    
            let tick_thread = std::thread::spawn(move || {
                let (lock, sleeper) = &*req_clone;
                let mut guard = lock.lock().unwrap();
                loop { 
                    while guard.is_empty() && !*are_we_being_dropped_clone.lock().unwrap() {
                        guard = sleeper.wait(guard).unwrap();
                    }
                    if *are_we_being_dropped_clone.lock().unwrap() {
                        break;
                    }
                    let mut start_time = guard.peek().unwrap().start_time;

                    while start_time > SystemTime::now() {
                        let timeout_time = start_time.duration_since(SystemTime::now()).unwrap();
                        guard = sleeper.wait_timeout(guard, timeout_time).unwrap().0;

                        if *are_we_being_dropped_clone.lock().unwrap() {
                            break;
                        }

                        start_time = guard.peek().unwrap().start_time;
                    }
                    if *are_we_being_dropped_clone.lock().unwrap() {
                        break;
                    }

                    //Either the timeout has elapsed or a new element came to the heap

                    let TickEntry {
                        start_time,
                        dur,
                        action_number,
                    } = guard.pop().unwrap();

                    let table_lock = action_table.lock().unwrap();
                    let specific_action = &table_lock[action_number];

                    specific_action();
                    guard.push(TickEntry{
                        start_time : start_time + dur,
                        dur : dur,
                        action_number : action_number,
                    });
                }
 
            });

            System {
                msges_go_here : msges_go_here, // This will be create ONCE
                executor_thread : Some(executor_thread),
                tick_thread : Some(tick_thread),
                tick_request_data : tick_request_data,

                last_action_number : 0,
                tick_actions : tick_actions,

                last_number : 0,
                are_we_being_dropped : are_we_being_dropped,
            }
        }
    }
    impl Drop for System {
        fn drop(&mut self) {
            self.msges_go_here.send(WorkerMsg::EndingMsg).unwrap();
            
            self.executor_thread.take().unwrap().join().unwrap();
            {
                *self.are_we_being_dropped.lock().unwrap().deref_mut() = true;
                let (_, sleeper) = &*self.tick_request_data.clone();
                sleeper.notify_one();
            }
            self.tick_thread.take().unwrap().join().unwrap();
        }
    }

    pub struct ModuleRef<T: 'static + Send> {
        module : Weak<Mutex<T>>,
        output_channel : Sender<WorkerMsg>,
        number : u128,
        how_many_instances : Arc<Mutex<usize>>,
    }

    impl<T : Send> ModuleRef<T > {
        pub fn force_send<M: Message>(&self, msg: M)
        where
            T: Handler<M>,
        {
            match self.module.upgrade() {
                Some(module) => module.lock().unwrap().deref_mut().handle(msg), 
                _ => (),
            }
        }
    }


    impl<T : Send> ModuleRef<T > {
        pub fn send<M: Message>(&self, msg: M)
        where
            T: Handler<M>,
        {
            let mod_ref = self.module.clone();
            let _ = self.output_channel.send(WorkerMsg::ExecuteMsg( 
                Box::new(move || {
                    match mod_ref.upgrade() {
                        Some(module) => module.lock().unwrap().deref_mut().handle(msg.clone()), 
                        _ => (),
                    }
                })
            ));
        }
    }

    impl<T : Send> fmt::Debug for ModuleRef<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
            f.write_str("<ModuleRef>")
        }
    }

    impl<T : Send> Clone for ModuleRef<T> {
        fn clone(&self) -> Self {
            (*self.how_many_instances.lock().unwrap().deref_mut()) += 1;
            ModuleRef {
                module : self.module.clone(),
                output_channel : self.output_channel.clone(),
                number : self.number,
                how_many_instances : self.how_many_instances.clone(),
            }
        }
    }

    impl<T : Send> Drop for ModuleRef<T> {
        fn drop(&mut self) {
            (*self.how_many_instances.lock().unwrap().deref_mut()) -= 1;
            if *self.how_many_instances.lock().unwrap() == 0 {
                let _ = self.output_channel.send(WorkerMsg::RemModule(self.number));
            }
        }
    }
}

pub mod system_setup_public {
    use crate::{Configuration, ModuleRef, ReliableBroadcast, StubbornBroadcast, System};
    use crate::build_stubborn_broadcast;
    use crate::build_reliable_broadcast;

    pub fn setup_system(
        system: &mut System,
        config: Configuration,
    ) -> ModuleRef<ReliableBroadcastModule> {
        let stub_broad = build_stubborn_broadcast(config.sender, config.processes.clone());
        let stub_broad_ref = system.register_module(StubbornBroadcastModule{
            stubborn_broadcast : stub_broad,
        });
        system.request_tick(&stub_broad_ref, config.retransmission_delay);

        let stab_stor = config.stable_storage;

        let rel_broad = build_reliable_broadcast(stub_broad_ref, stab_stor, config.self_process_identifier, config.processes.clone().len(), config.delivered_callback);

        system.register_module(ReliableBroadcastModule {
            reliable_broadcast : rel_broad,
        })
    }

    pub struct ReliableBroadcastModule {
        pub(crate) reliable_broadcast: Box<dyn ReliableBroadcast>,
    }

    pub struct StubbornBroadcastModule {
        pub(crate) stubborn_broadcast: Box<dyn StubbornBroadcast>,
    }
}
