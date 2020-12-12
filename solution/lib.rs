mod domain;

pub use crate::broadcast_public::*;
pub use crate::executors_public::*;
pub use crate::stable_storage_public::*;
pub use crate::system_setup_public::*;
pub use domain::*;

// use log::{info, warn};

pub mod broadcast_public {
    use crate::executors_public::ModuleRef;
    use crate::{PlainSenderMessage, StableStorage, StubbornBroadcastModule, SystemAcknowledgmentMessage, SystemBroadcastMessage, SystemMessageContent, SystemMessageHeader, SystemMessage};
    use std::collections::{HashSet, HashMap};
    use uuid::Uuid;
    use std::str::*;

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
        (Uuid::from_bytes(fst), deserialize_hdr(bytes.drain(1..(TRANSFORM_BYTES_SIZE - 1)).collect()))
    }
    fn serialize_msg(msg : SystemMessage) -> Vec<u8> {
        let mut vec = Vec::new();
        vec.extend(serialize_hdr(msg.header));
        vec.extend(msg.data.msg);
        vec
    }
    fn deserialize_msg(mut bytes : Vec<u8>) -> SystemMessage {
        let hdr = deserialize_hdr(bytes.clone());
        let content = SystemMessageContent{
            msg : bytes.drain(1..(2*TRANSFORM_BYTES_SIZE - 1)).collect(),
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

    impl MyReliableBroadcast {

        fn retrieve_pending(&mut self) {
            match self.storage.get("keys_pending") {
                Some(mut pending_bytes) => {
                    let mut new_pending : HashSet<(Uuid, SystemMessageHeader)> = HashSet::new();
                    while pending_bytes.clone().len() > 0 {
                        if pending_bytes.len() < 3 * TRANSFORM_BYTES_SIZE {
                            log::warn!("Invalid number of bytes when trying to retrieve pending")
                        }
                        else {
                            let (uuid, hdr) = deserialize_hdr_with_uuid(pending_bytes.clone());
                            pending_bytes.drain(1..(2 * TRANSFORM_BYTES_SIZE - 1)); 
                            new_pending.insert((uuid, hdr));
                        }
                    }
                    self.pending = new_pending 
                }
                None => {}
            }
        }
        fn store_pending(&mut self) {
            let mut keys_pending = Vec::new();
            for (uuid, hdr) in self.pending.clone() {
                keys_pending.extend(serialize_hdr_with_uuid(uuid, hdr));
            }
            self.storage.put("keys_pending", &keys_pending).unwrap();
        }
        fn retrieve_delivered(&mut self) {
            match self.storage.get("keys_delivered") {
                Some(mut delivered_bytes) => {
                    let mut new_delivered : HashSet<(Uuid, SystemMessageHeader)> = HashSet::new();
                    while delivered_bytes.clone().len() > 0 {
                        if delivered_bytes.len() < 3 * TRANSFORM_BYTES_SIZE {
                            log::warn!("Invalid number of bytes when trying to retrieve delivered")
                        }
                        else {
                            let (uuid, hdr) = deserialize_hdr_with_uuid(delivered_bytes.clone());
                            delivered_bytes.drain(1..(2 * TRANSFORM_BYTES_SIZE - 1)); 
                            new_delivered.insert((uuid, hdr));
                        }
                    }
                    self.delivered = new_delivered
                }
                None => {}
            }
        }
        fn store_delivered(&mut self) {
            let mut keys_delivered = Vec::new();
            for (id, hdr) in self.delivered.clone() {
                keys_delivered.extend(serialize_hdr_with_uuid(id, hdr));
            }
            self.storage.put("keys_delivered", &keys_delivered).unwrap();
        }
        fn retrieve_actual_msges(&mut self) {
            match self.storage.get("keys_actual_msges") {
                Some(mut actual_msges_bytes) => {
                    let mut new_actual_msges : HashMap<SystemMessageHeader, SystemMessage> = HashMap::new();

                    while actual_msges_bytes.clone().len() > 0 {
                        if actual_msges_bytes.len() < 2 * TRANSFORM_BYTES_SIZE {
                            log::warn!("Invalid number of bytes when trying to retrieve actual msges")
                        }
                        else {
                            let hdr = deserialize_hdr(actual_msges_bytes.clone());
                            actual_msges_bytes.drain(1..(2 * TRANSFORM_BYTES_SIZE - 1)); 
                            new_actual_msges.insert(
                                hdr, 
                                deserialize_msg(self.storage.get(
                                    from_utf8(&serialize_hdr(hdr)).unwrap()
                                ).unwrap())
                            );
                        }
                    }
                    self.actual_msges = new_actual_msges
                }
                None => {}
            }
        }
        fn store_actual_msges(&mut self) {
            let mut keys_msges = Vec::new();
            for (hdr, msg) in self.actual_msges.clone() {
                keys_msges.extend(serialize_hdr(hdr));
                self.storage.put(from_utf8(&serialize_hdr(hdr)).unwrap(), &serialize_msg(msg)).unwrap();
            }
            self.storage.put("keys_actual_msges", &keys_msges).unwrap();
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
                    msg : msg.msg.clone(), // Will this work with instances on different machines?
                },
            };
            self.pending.insert((self.id, system_msg.header));
            self.store_pending();
            self.store_actual_msges();

            self.stubborn.send(SystemBroadcastMessage {
                forwarder_id : self.id,
                message : system_msg.clone(),
            });
            self.actual_msges.insert(system_msg.header, system_msg);
            self.current_msg_number += 1;
            self.store_current_msg_number();
            // self.try_to_deliver();
        }

        fn deliver_message(&mut self, msg: SystemBroadcastMessage) {
            let source_id = msg.clone().message.header.message_source_id;
            let forwarder_id = msg.forwarder_id;
            let entry = (source_id, msg.clone().message.header);

            if !self.pending.contains(&entry) {
                self.pending.insert(entry);
                self.stubborn.send(SystemBroadcastMessage {
                    forwarder_id : self.id,
                    message : msg.message.clone(),
                }); // Unleash the storm!

                let ack_set = self.ack.get_mut(&msg.message.header).unwrap(); 
                if !ack_set.contains(&forwarder_id) {
                    ack_set.insert(forwarder_id);

                    let hdr = msg.message.header;
                    let new_delivered_entry = (source_id, hdr);
                    if !self.delivered.contains(&new_delivered_entry) && ack_set.len() > self.how_many_proc / 2 {
                        let full_msg = self.actual_msges.get(&hdr).unwrap();
                        (*self.callback)(full_msg.clone());
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
            stubborn : sbeb,
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
            self.messages.insert(msg.clone().message.header.message_id, msg.clone());
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
    const MAX_FILE_NAME : usize = 255;
    const MAX_FILE_CONTENT : usize = 65536;
    
    pub trait StableStorage: Send {
        fn put(&mut self, key: &str, value: &[u8]) -> Result<(), String>;

        fn get(&self, key: &str) -> Option<Vec<u8>>;
    }
    struct MyStableStorage {
        dir : PathBuf,
    }
    impl StableStorage for MyStableStorage {
        fn put(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
            if key.len() > MAX_FILE_NAME || value.len() > MAX_FILE_CONTENT {
                return Err("Too long name or too long content.".to_string());
            } 
            let mut path_to_tmp = self.dir.clone();
            path_to_tmp.push("not_yet_inserted");
            path_to_tmp.push(key);
            if path_to_tmp.is_file() {
                std::fs::remove_file(path_to_tmp.clone()).unwrap();
            }
            let mut file = std::fs::OpenOptions::new().create(true).write(true).open(path_to_tmp.clone()).unwrap();

            file.write_all(value).unwrap();
            let mut path_to_normal = self.dir.clone();
            path_to_normal.push(key);
            Ok(rename(path_to_tmp.clone(), path_to_normal.clone()).unwrap())

        }
        fn get(&self, key: &str) -> Option<Vec<u8>> {
            if key.len() > MAX_FILE_NAME {
                return None;
            }
            let content : String;
            let mut file_path = self.dir.clone();
            file_path.push(key);
            if !file_path.is_file() {
                return None;
            }
            content = read_to_string(file_path).unwrap();
            return Some(content.as_bytes().to_vec());
        }
    }

    pub fn build_stable_storage(_root_storage_dir: PathBuf) -> Box<dyn StableStorage> {
        let mut tmp_path = _root_storage_dir.clone();
        tmp_path.push("not_yet_inserted"); // This will be necessary so our put is atomic
        create_dir_all(tmp_path.clone()).unwrap();
        Box::new(MyStableStorage {
            dir : _root_storage_dir,
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
    use uuid::Uuid;
    use std::collections::HashMap;



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
        action_number : i32,
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
    
    // struct LittleMsg {
    //     msg : Box<dyn Message>,
    // }
    
    // pub type ClosureMsg<T> = Box<Fn(ModuleRef<T>) -> () + Send>;
    pub type ClosureMsg = Box<dyn Fn() -> () + Send>;
    // pub type ClosureMsgBoxless = Fn() -> () + Send;
    // pub trait MyTrait: Send + 'static + Sized {}
    
    // struct LittleBox<T : Send + 'static + ?Sized> {
    //     mod_ref : Box<ModuleRef<T>>,
    // }

    pub enum WorkerMsg {
        // NewRbebModule((Uuid, Box<Fn() -> ModuleRef<dyn Send + 'static>>)),
        NewRbebModule((Uuid, Box<dyn Fn() -> () + Send>)),
        ExecuteMsg(ClosureMsg),
    }
    
    pub struct System {
        // stubborns : Vec<Sender<ClosureMsg<ModuleRef<StubbornBroadcastModule>>>>,
        // pub reliables_msges : Vec<Sender<ClosureMsg<ModuleRef<ReliableBroadcastModule>>>>,
        // stuff : Box<ModuleRef<Send + 'static>>,
        // pub reliables : Arc<Mutex<HashMap<Uuid, ModuleRef<dyn Send + 'static>>>>,
        // pub reliables_reverted : HashMap<ModuleRef<ReliableBroadcastModule>, Uuid >,
        pub msges_go_here : Sender<WorkerMsg>, // This will be create ONCE
        pub msg_queue : Receiver<WorkerMsg>,
        // pub channels_to_reliables : Sender<ClosureMsg<ReliableBroadcastModule>>, 
        // processes : Vec<>,
        // no_msg_control : Arc<(Mutex<BinaryHeap <TickEntry> >, Condvar)>,
        executor_thread : Option<JoinHandle<()>>,
        tick_thread : Option<JoinHandle<()>>,
        last_number : u128,

        last_action_number : i32,
        tick_actions : Arc<Mutex<HashMap<i32, ClosureMsg>>>,
        
        tick_request_data : Arc<(Mutex<BinaryHeap<TickEntry>>, Condvar)>,   
    }


    impl System {
        pub fn request_tick<T: Handler<Tick> + Send>(&mut self, requester: &ModuleRef<T>, _delay: Duration) {
            let (tick_queue, sleeper) = &*self.tick_request_data;
            let req_clone = requester.clone();

            let mut tick_queue_locked = tick_queue.lock().unwrap(); 
            let inserted_start_time = SystemTime::now();


            self.tick_actions.lock().unwrap().insert(self.last_action_number, Box::new(
                move || {
                    req_clone.force_send(Tick{});
                }
            ));
            println!("Created an action at nr.{}", self.last_action_number);
            println!("With starttime = {:?}", inserted_start_time);

            tick_queue_locked.push(TickEntry{
                start_time : inserted_start_time,
                dur : _delay,
                // module_ref : requester.clone(), 
                // action : Box::new(move || req_clone.clone().send(Tick{}) ),
                action_number : self.last_action_number
            });

            self.last_action_number = self.last_action_number + 1;

            if tick_queue_locked.peek().unwrap().start_time == inserted_start_time {
                sleeper.notify_one();
            }
        }

        pub fn register_module<T : Send + 'static> (&mut self, module: T) -> ModuleRef<T> {
            
            let module_ref = ModuleRef {
                number : self.last_number,
                module : Arc::new(Mutex::new(module)),
                output_channel : self.msges_go_here.clone(),
            };

            self.last_number = self.last_number + 1;

            module_ref
        }

        pub fn new() -> Self {
            let (msges_go_here, msg_queue): (Sender<WorkerMsg>, Receiver<WorkerMsg>) = unbounded();
            let (tx_ref, rx_ref) = (msges_go_here.clone(), msg_queue.clone());
            let tick_request_data : Arc<(Mutex<BinaryHeap<TickEntry>>, Condvar)> = Arc::new((Mutex::new(BinaryHeap::new()), Condvar::new()));   

            let tick_actions : Arc<Mutex<HashMap<i32, ClosureMsg>>> = Arc::new(Mutex::new(HashMap::new()));

            // self.stubborns.push(tx);

            let executor_thread = std::thread::spawn(move || { 
                while let Ok(msg) = msg_queue.recv() {
                    match msg {
                        WorkerMsg::NewRbebModule((id, module_ref)) => {
                            println!("New module created!");
                            // *Rc::get_mut(& mut reliables).unwrap().get_mut().unwrap().get_mut(&id).unwrap() = module_ref;

                            // *(reliables.clone()).lock().unwrap().get_mut(&id).unwrap() = module_ref;
                        },
                        WorkerMsg::ExecuteMsg(closure) => {
                            println!("ExecuteMsg!");
                            closure();
                        },
                    }
                }
            });
            let req_clone = tick_request_data.clone();
            let action_table = tick_actions.clone();
            let tick_thread = std::thread::spawn(move || {
                loop { 
                    {
                        let (lock, sleeper) = &*req_clone.clone();
                        let mut guard = lock.lock().unwrap();
                        while guard.is_empty() {
                            guard = sleeper.wait(guard).unwrap();
                        }

                        let start_time = guard.peek().unwrap().start_time;
                        if start_time > SystemTime::now() {
                            let timeout_time = start_time.duration_since(SystemTime::now()).unwrap();
                            let _result = sleeper.wait_timeout(guard, timeout_time).unwrap();
                        }
                    }
                    //Either the timeout has elapsed or a new element came to the heap

                    let (lock, _) = &*req_clone.clone();
                    let mut guard = lock.lock().unwrap();


                    let TickEntry {
                        start_time,
                        dur,
                        action_number,
                    } = guard.pop().unwrap();

                    println!("Got myself an action nr.{}", action_number);
                    println!("And now is {:?}", SystemTime::now());
                    println!("With start_time = {:?}", start_time);

                    let table_lock = action_table.lock().unwrap();
                    let specific_action = table_lock.get(&action_number).unwrap();
                    // let specific_action = val.lock().unwrap(); 

                    specific_action();
                    guard.push(TickEntry{
                        start_time : start_time + dur,
                        dur : dur,
                        action_number : action_number,
                        // action : action,
                    });
                }
 
            });

            System {
                msges_go_here : tx_ref, // This will be create ONCE
                msg_queue : rx_ref,
                executor_thread : Some(executor_thread),
                tick_thread : Some(tick_thread),
                tick_request_data : tick_request_data,

                last_action_number : 0,
                tick_actions : tick_actions,

                last_number : 0,
            }
        }

        #[allow(dead_code)]
        fn drop(&mut self) {
            println!("Drop!");
            self.executor_thread.take().unwrap().join().unwrap();
            self.tick_thread.take().unwrap().join().unwrap();
        }
    }

    pub struct ModuleRef<T: 'static + Send> {
        module : Arc<Mutex<T>>,
        output_channel : Sender<WorkerMsg>,
        number : u128,
    }

    impl<T : Send> ModuleRef<T > {
        pub fn force_send<M: Message>(&self, msg: M)
        where
            T: Handler<M>,
        {
            self.module.lock().unwrap().deref_mut().handle(msg);
        }
    }


    impl<T : Send> ModuleRef<T > {
        pub fn send<M: Message>(&self, msg: M)
        where
            T: Handler<M>,
        {
            let mod_ref = self.module.clone();
            self.output_channel.send(WorkerMsg::ExecuteMsg(
                Box::new(move || {
                    mod_ref.lock().unwrap().deref_mut().handle(msg.clone());
                })
            )).unwrap();
        }
    }

    impl<T : Send> fmt::Debug for ModuleRef<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
            f.write_str("<ModuleRef>")
        }
    }

    impl<T : Send> Clone for ModuleRef<T> {
        fn clone(&self) -> Self {
            ModuleRef {
                module : self.module.clone(),
                output_channel : self.output_channel.clone(),
                number : self.number,
            }
        }
    }
}

pub mod system_setup_public {
    use crate::{Configuration, ModuleRef, ReliableBroadcast, StubbornBroadcast, System};
    use crate::build_stubborn_broadcast;
    use crate::build_reliable_broadcast;

    pub fn setup_system(
        _system: &mut System,
        _config: Configuration,
    ) -> ModuleRef<ReliableBroadcastModule> {
        let stub_broad = build_stubborn_broadcast(_config.sender, _config.processes.clone());
        let stub_broad_ref = _system.register_module(StubbornBroadcastModule{
            stubborn_broadcast : stub_broad,
        });
        _system.request_tick(&stub_broad_ref, _config.retransmission_delay);

        let stab_stor = _config.stable_storage;
        // let stab_stor_ref = _system.register_module(stab_stor);

        let rel_broad = build_reliable_broadcast(stub_broad_ref.clone(), stab_stor, _config.self_process_identifier, _config.processes.clone().len(), _config.delivered_callback);
        let rbeb_module_ref = _system.register_module(ReliableBroadcastModule {
            reliable_broadcast : rel_broad,
        });


        rbeb_module_ref
    }

    pub struct ReliableBroadcastModule {
        pub(crate) reliable_broadcast: Box<dyn ReliableBroadcast>,
    }

    pub struct StubbornBroadcastModule {
        pub(crate) stubborn_broadcast: Box<dyn StubbornBroadcast>,
    }
}
